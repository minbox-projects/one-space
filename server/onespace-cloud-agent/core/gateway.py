#!/usr/bin/env python3
"""
onespace-cloud-agent: Linux AI 网关核心组件
提供轻量、高可用、低内存开销的 OpenAI 兼容代理服务。
特性：
1. 纯 Python 3 标准库实现，无需第三方外部依赖，常驻内存 < 35MB。
2. 多服务商轮询与故障转移（Failover & Key Cooldown）。
3. 模型池（Model Pool）虚拟映射与降级机制。
4. 服务商模板自动同步（Auto-Sync）。
5. 支持流式 SSE (Server-Sent Events) 与非流式请求转发。
"""

import http.server
import json
import logging
import os
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from typing import Any, Dict, List, Optional, Tuple

logger = logging.getLogger("onespace-gateway")


class ProviderState:
    def __init__(self, name: str, config: Dict[str, Any]):
        self.name = name
        self.base_url = config.get("base_url", "").rstrip("/")
        self.api_keys: List[str] = config.get("api_keys", [])
        self.models: Dict[str, Any] = config.get("models", {})
        self.is_active: bool = config.get("is_active", True)
        self.template_url: Optional[str] = config.get("template_url")
        
        # 轮询游标与冷却状态 {api_key: cooldown_until_timestamp}
        self.key_index = 0
        self.key_cooldowns: Dict[str, float] = {}
        self.lock = threading.Lock()

    def get_available_key(self) -> Optional[str]:
        with self.lock:
            now = time.time()
            if not self.api_keys:
                return None
            
            # 清理过期的冷却
            self.key_cooldowns = {k: v for k, v in self.key_cooldowns.items() if v > now}
            
            # 尝试轮询未冷却的 key
            num_keys = len(self.api_keys)
            for _ in range(num_keys):
                candidate = self.api_keys[self.key_index % num_keys]
                self.key_index += 1
                if candidate not in self.key_cooldowns:
                    return candidate
            
            # 如果全部在冷却，返回即将到期的那个或第一个
            logger.warning(f"[{self.name}] 所有 API Key 均处于冷却中，尝试复用第一个")
            return self.api_keys[0]

    def mark_key_failed(self, key: str, cooldown_seconds: float = 60.0):
        with self.lock:
            self.key_cooldowns[key] = time.time() + cooldown_seconds
            logger.warning(f"[{self.name}] Key ...{key[-6:] if len(key) > 6 else key} 失败，冷却 {cooldown_seconds}s")


class GatewayEngine:
    def __init__(self, config_path: str, sync_opencode: bool = True, opencode_config_path: Optional[str] = None):
        self.config_path = config_path
        self.sync_opencode = sync_opencode
        self.opencode_config_path = opencode_config_path
        self.providers: Dict[str, ProviderState] = {}
        self.model_pools: Dict[str, Dict[str, Any]] = {}
        self.global_port = 17688
        self.auto_sync_interval = 3600
        self.running = False
        self.load_config()

    def load_config(self):
        if not os.path.exists(self.config_path):
            logger.warning(f"配置文件未找到: {self.config_path}，使用空配置")
            return

        with open(self.config_path, "r", encoding="utf-8") as f:
            cfg = json.load(f)

        gw_cfg = cfg.get("ai_gateway", {})
        self.global_port = gw_cfg.get("port", 17688)
        self.auto_sync_interval = gw_cfg.get("auto_sync_interval_seconds", 3600)

        # 加载服务商
        providers_data = gw_cfg.get("providers", {})
        new_providers = {}
        for p_name, p_data in providers_data.items():
            new_providers[p_name] = ProviderState(p_name, p_data)
        self.providers = new_providers

        # 加载模型池
        self.model_pools = gw_cfg.get("model_pools", {})
        logger.info(f"配置加载完成: {len(self.providers)} 个服务商, {len(self.model_pools)} 个模型池, 端口 {self.global_port}")
        if self.sync_opencode:
            self.sync_to_opencode_config()

    def resolve_candidates(self, requested_model: str) -> List[Tuple[str, str]]:
        """
        将请求的模型名称解析为一个或多个候选 (provider_name, target_model) 列表。
        按优先级排序，以便于重试和降级。
        """
        candidates: List[Tuple[str, str]] = []

        # 1. 命中模型池 (Model Pool)
        if requested_model in self.model_pools:
            pool = self.model_pools[requested_model]
            pool_candidates = pool.get("candidates", [])
            for c in pool_candidates:
                p_name = c.get("provider")
                m_name = c.get("model")
                if p_name in self.providers and self.providers[p_name].is_active:
                    provider = self.providers[p_name]
                    upstream = provider.models.get(m_name, {}).get("upstream_model", m_name) if isinstance(provider.models.get(m_name), dict) else m_name
                    candidates.append((p_name, upstream))
            if candidates:
                return candidates

        # 2. 命中具体服务商的前缀格式：provider/model（如 deepseek/deepseek-chat）
        if "/" in requested_model:
            parts = requested_model.split("/", 1)
            p_name, m_name = parts[0], parts[1]
            if p_name in self.providers and self.providers[p_name].is_active:
                provider = self.providers[p_name]
                upstream = provider.models.get(m_name, {}).get("upstream_model", m_name) if isinstance(provider.models.get(m_name), dict) else m_name
                return [(p_name, upstream)]

        # 3. 遍历各服务商是否有直接对应的模型映射
        for p_name, provider in self.providers.items():
            if not provider.is_active:
                continue
            if requested_model in provider.models:
                candidates.append((p_name, provider.models[requested_model].get("upstream_model", requested_model)))

        # 4. 兜底回退：如果都未精确匹配，尝试在所有有效服务商中透传同名模型
        if not candidates:
            for p_name, provider in self.providers.items():
                if provider.is_active:
                    candidates.append((p_name, requested_model))

        return candidates

    def sync_templates(self):
        """同步服务商模板中声明的在线模型"""
        logger.info("开始执行服务商模板模型自动同步...")
        for name, provider in self.providers.items():
            if not provider.template_url:
                continue
            try:
                req = urllib.request.Request(
                    provider.template_url,
                    headers={"User-Agent": "OneSpace-Cloud-Agent/1.0", "Accept": "application/json"}
                )
                with urllib.request.urlopen(req, timeout=15) as resp:
                    if resp.status == 200:
                        data = json.loads(resp.read().decode("utf-8"))
                        raw_models = data.get("data", []) or data.get("models", [])
                        count = 0
                        for item in raw_models:
                            m_id = item.get("id") if isinstance(item, dict) else str(item)
                            if m_id and m_id not in provider.models:
                                provider.models[m_id] = {"upstream_model": m_id}
                                count += 1
                        logger.info(f"[{name}] 模板同步成功，新增 {count} 个模型")
            except Exception as e:
                logger.warning(f"[{name}] 模板同步失败: {e}")
        if self.sync_opencode:
            self.sync_to_opencode_config()

    def sync_to_opencode_config(self, target_file: Optional[str] = None):
        """自动将当前网关可用的模型列表注入到 OpenCode 的主配置文件中，与 OneSpace App 保持完全一致"""
        target_path = target_file or self.opencode_config_path
        if not self.sync_opencode and not target_path:
            return

        try:
            if target_path:
                opencode_cfg_file = target_path
                os.makedirs(os.path.dirname(os.path.abspath(opencode_cfg_file)), exist_ok=True)
            else:
                cfg_dir = os.path.expanduser("~/.config/opencode")
                os.makedirs(cfg_dir, exist_ok=True)
                opencode_cfg_file = os.path.join(cfg_dir, "opencode.json")
                jsonc_file = os.path.join(cfg_dir, "opencode.jsonc")

                if os.path.exists(jsonc_file):
                    try:
                        os.remove(jsonc_file)
                    except Exception:
                        pass

            current_cfg = {}
            if os.path.exists(opencode_cfg_file):
                try:
                    with open(opencode_cfg_file, "r", encoding="utf-8") as f:
                        current_cfg = json.load(f)
                except Exception:
                    pass

            if "$schema" not in current_cfg:
                current_cfg["$schema"] = "https://opencode.ai/config.json"

            # 整理网关当前模型列表（与 OneSpace 桌面端结构完全对齐）
            models_dict = {}

            # 1. 优先放入虚拟模型池
            for pool_id, pool_meta in self.model_pools.items():
                models_dict[pool_id] = {
                    "name": pool_meta.get("description", pool_id),
                    "limit": {"context": 1000000, "output": 0}
                }

            # 2. 放入各活跃服务商模型（保持纯净模型标识与显示名）
            for p_name, provider in self.providers.items():
                if not provider.is_active:
                    continue
                for m_id, m_meta in provider.models.items():
                    if m_id in models_dict:
                        continue
                    disp_name = m_id
                    ctx_window = 1000000
                    reasoning_efforts = []

                    if isinstance(m_meta, dict):
                        disp_name = m_meta.get("display_name", m_id)
                        ctx_window = m_meta.get("context_window", 1000000) or 1000000
                        reasoning_efforts = m_meta.get("reasoning_efforts", [])

                    model_entry = {
                        "name": disp_name,
                        "limit": {"context": ctx_window, "output": 0}
                    }
                    if reasoning_efforts:
                        model_entry["reasoning"] = True
                        model_entry["variants"] = {
                            rf: {"reasoningEffort": rf} for rf in reasoning_efforts
                        }

                    models_dict[m_id] = model_entry

            # 与 OneSpace App 保持严格一致：只写入且仅保留一个 "Cloud Agent Gateway" 提供商
            current_cfg["provider"] = {
                "gateway": {
                    "name": "Cloud Agent Gateway",
                    "npm": "@ai-sdk/openai-compatible",
                    "options": {
                        "baseURL": f"http://127.0.0.1:{self.global_port}/v1",
                        "apiKey": "sk-onespace-cloud-agent"
                    },
                    "models": models_dict
                }
            }

            if not current_cfg.get("model") and "cloud-coder" in self.model_pools:
                current_cfg["model"] = "gateway/cloud-coder"

            with open(opencode_cfg_file, "w", encoding="utf-8") as f:
                json.dump(current_cfg, f, ensure_ascii=False, indent=2)

            logger.info(f"已按 OneSpace 规范同步 {len(models_dict)} 个模型到 OpenCode 终端配置 (单 Cloud Agent Gateway 提供商): {opencode_cfg_file}")
        except Exception as e:
            logger.warning(f"同步模型到 OpenCode 终端配置失败: {e}")

    def start_background_tasks(self):
        def _sync_loop():
            while self.running:
                try:
                    self.sync_templates()
                except Exception as e:
                    logger.error(f"后台同步出现异常: {e}")
                for _ in range(max(1, self.auto_sync_interval)):
                    if not self.running:
                        break
                    time.sleep(1)

        t = threading.Thread(target=_sync_loop, daemon=True, name="gateway-template-sync")
        t.start()


class GatewayRequestHandler(http.server.BaseHTTPRequestHandler):
    gateway: GatewayEngine = None

    def log_message(self, format, *args):
        pass

    def send_json_response(self, status: int, data: Any):
        body = json.dumps(data, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Headers", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.end_headers()
        self.wfile.write(body)

    def do_OPTIONS(self):
        self.send_response(204)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Headers", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.end_headers()

    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        path = parsed.path

        if path in ("/health", "/v1/health"):
            self.send_json_response(200, {
                "status": "ok",
                "providers_count": len(self.gateway.providers),
                "model_pools": list(self.gateway.model_pools.keys())
            })
            return

        if path in ("/models", "/v1/models"):
            models_list = []
            for pool_id, pool_meta in self.gateway.model_pools.items():
                models_list.append({
                    "id": pool_id,
                    "object": "model",
                    "created": int(time.time()),
                    "owned_by": "onespace-model-pool",
                    "description": pool_meta.get("description", "Virtual Model Pool")
                })
            for p_name, provider in self.gateway.providers.items():
                for m_id in provider.models.keys():
                    models_list.append({
                        "id": f"{p_name}/{m_id}",
                        "object": "model",
                        "created": int(time.time()),
                        "owned_by": p_name
                    })

            self.send_json_response(200, {"object": "list", "data": models_list})
            return

        self.send_json_response(404, {"error": {"message": f"未找到端点: {path}"}})

    def do_POST(self):
        parsed = urllib.parse.urlparse(self.path)
        path = parsed.path

        if path not in ("/chat/completions", "/v1/chat/completions"):
            self.send_json_response(404, {"error": {"message": f"不支持的端点: {path}"}})
            return

        content_length = int(self.headers.get("Content-Length", 0))
        if content_length <= 0:
            self.send_json_response(400, {"error": {"message": "请求体为空"}})
            return

        try:
            req_body_bytes = self.rfile.read(content_length)
            req_data = json.loads(req_body_bytes.decode("utf-8"))
        except Exception as e:
            self.send_json_response(400, {"error": {"message": f"请求 JSON 解析失败: {e}"}})
            return

        requested_model = req_data.get("model", "")
        candidates = self.gateway.resolve_candidates(requested_model)

        if not candidates:
            self.send_json_response(502, {
                "error": {
                    "message": f"没有可用的服务商为模型 '{requested_model}' 提供服务，请检查网关配置。"
                }
            })
            return

        is_stream = req_data.get("stream", False) is True

        last_error_message = "未知错误"
        for idx, (p_name, target_model) in enumerate(candidates):
            provider = self.gateway.providers.get(p_name)
            if not provider or not provider.is_active:
                continue

            api_key = provider.get_available_key()
            if not api_key:
                logger.warning(f"[{p_name}] 无可用 API Key，跳过")
                continue

            req_data["model"] = target_model
            forward_body = json.dumps(req_data, ensure_ascii=False).encode("utf-8")

            upstream_url = f"{provider.base_url}/chat/completions"
            if not upstream_url.startswith("http"):
                upstream_url = "https://" + upstream_url

            logger.info(f"请求转发: model={requested_model} -> [{p_name}] model={target_model} (尝试 {idx+1}/{len(candidates)})")

            req = urllib.request.Request(
                upstream_url,
                data=forward_body,
                headers={
                    "Content-Type": "application/json",
                    "Authorization": f"Bearer {api_key}",
                    "Accept": "text/event-stream" if is_stream else "application/json",
                    "User-Agent": "OneSpace-Cloud-Gateway/1.0"
                },
                method="POST"
            )

            try:
                resp = urllib.request.urlopen(req, timeout=120)
                status_code = resp.status

                if is_stream:
                    self.send_response(status_code)
                    for k, v in resp.getheaders():
                        if k.lower() in ("content-type", "cache-control", "connection"):
                            self.send_header(k, v)
                    self.send_header("Access-Control-Allow-Origin", "*")
                    self.end_headers()

                    try:
                        while True:
                            chunk = resp.read(1024)
                            if not chunk:
                                break
                            self.wfile.write(chunk)
                            self.wfile.flush()
                    except (BrokenPipeError, ConnectionResetError):
                        logger.info("下游客户端主动断开连接")
                    return
                else:
                    resp_body = resp.read()
                    self.send_response(status_code)
                    self.send_header("Content-Type", "application/json; charset=utf-8")
                    self.send_header("Content-Length", str(len(resp_body)))
                    self.send_header("Access-Control-Allow-Origin", "*")
                    self.end_headers()
                    self.wfile.write(resp_body)
                    return

            except urllib.error.HTTPError as e:
                err_text = ""
                try:
                    err_text = e.read().decode("utf-8")
                except Exception:
                    pass
                last_error_message = f"[{p_name}] HTTP {e.code}: {err_text or e.reason}"
                logger.warning(f"转发失败: {last_error_message}")

                if e.code in (429, 500, 502, 503, 504):
                    provider.mark_key_failed(api_key, cooldown_seconds=60.0)
                    continue
                else:
                    self.send_json_response(e.code, {
                        "error": {
                            "message": f"上游服务商业务报错: {err_text or e.reason}",
                            "provider": p_name,
                            "upstream_code": e.code
                        }
                    })
                    return

            except Exception as e:
                last_error_message = f"[{p_name}] 网络传输异常: {e}"
                logger.warning(f"转发网络异常: {last_error_message}")
                provider.mark_key_failed(api_key, cooldown_seconds=30.0)
                continue

        self.send_json_response(502, {
            "error": {
                "message": f"所有上游候选均重试失败。最后错误: {last_error_message}"
            }
        })


def run_gateway_server(config_path: str, host: str = "127.0.0.1"):
    gateway = GatewayEngine(config_path)
    gateway.running = True
    gateway.start_background_tasks()

    class BoundHandler(GatewayRequestHandler):
        pass
    BoundHandler.gateway = gateway

    server_address = (host, gateway.global_port)
    httpd = http.server.ThreadingHTTPServer(server_address, BoundHandler)
    logger.info(f"OneSpace Linux AI Gateway 启动就绪: http://{host}:{gateway.global_port}")

    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        logger.info("收到终止信号，正在关闭网关...")
    finally:
        gateway.running = False
        httpd.server_close()


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, format="%(asctime)s [%(levelname)s] %(message)s")
    import sys
    cfg_file = sys.argv[1] if len(sys.argv) > 1 else "config.json"
    run_gateway_server(cfg_file)
