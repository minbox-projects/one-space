#!/usr/bin/env python3
"""
onespace-cloud-agent: Linux AI 网关核心组件
提供轻量、高可用、低内存开销的 OpenAI 兼容代理服务。
特性：
1. 纯 Python 3 标准库实现，无需第三方外部依赖，保持轻量的运行开销。
2. 多服务商轮询与故障转移（Failover & Key Cooldown）。
3. 模型池（Model Pool）虚拟映射与降级机制。
4. 服务商模板自动同步（Auto-Sync）。
5. 支持流式 SSE (Server-Sent Events) 与非流式请求转发。
"""

import hmac
import http.server
import json
import logging
import math
import os
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from typing import Any, Callable, Dict, List, Optional, Tuple

logger = logging.getLogger("onespace-gateway")

DEFAULT_AUTH_TOKEN = "sk-onespace-cloud-agent"
DEFAULT_MAX_BODY_BYTES = 10485760
DEFAULT_HTTP_COOLDOWN_SECONDS = 60.0
DEFAULT_TRANSPORT_COOLDOWN_SECONDS = 30.0
DEFAULT_MAX_COOLDOWN_SECONDS = 600.0
STREAM_READ_SIZE = 65536


class ProviderKeyState:
    """Shared mutable key rotation state for one provider."""

    def __init__(self, clock: Optional[Callable[[], float]] = None) -> None:
        self.clock = clock
        self.key_index = 0
        self.key_cooldowns: Dict[str, float] = {}
        self.lock = threading.Lock()


class ProviderState:
    def __init__(
        self,
        name: str,
        config: Dict[str, Any],
        key_state: Optional[ProviderKeyState] = None,
        clock: Optional[Callable[[], float]] = None,
    ):
        self.name = name
        self.base_url = str(config.get("base_url", "")).rstrip("/")
        self.api_keys: List[str] = list(config.get("api_keys", []) or [])
        models = config.get("models", {})
        self.models: Dict[str, Any] = models if isinstance(models, dict) else {}
        self.is_active: bool = config.get("is_active", True)
        self.template_url: Optional[str] = config.get("template_url")

        # Key rotation and cooldown state is shared across catalog generations so
        # template synchronization never resets runtime availability.
        if key_state is None:
            key_state = ProviderKeyState(clock)
        elif clock is not None and key_state.clock is None:
            key_state.clock = clock
        self.key_state = key_state
        self.clock = self.key_state.clock

    def with_models(self, models: Dict[str, Any]) -> "ProviderState":
        return ProviderState(
            self.name,
            {
                "base_url": self.base_url,
                "api_keys": self.api_keys,
                "models": models,
                "is_active": self.is_active,
                "template_url": self.template_url,
            },
            key_state=self.key_state,
            clock=self.clock,
        )

    def _now(self) -> float:
        clock = self.key_state.clock
        return clock() if clock is not None else time.monotonic()

    def _prune_cooldowns(self, now: float) -> None:
        self.key_state.key_cooldowns = {
            k: v for k, v in self.key_state.key_cooldowns.items() if v > now
        }

    def get_available_key(self) -> Optional[str]:
        with self.key_state.lock:
            now = self._now()
            if not self.api_keys:
                return None

            # 清理过期的冷却
            self._prune_cooldowns(now)

            # 尝试轮询未冷却的 key；已冷却的 key 绝不复用
            num_keys = len(self.api_keys)
            for _ in range(num_keys):
                candidate = self.api_keys[self.key_state.key_index % num_keys]
                self.key_state.key_index += 1
                if candidate not in self.key_state.key_cooldowns:
                    return candidate

            logger.warning(f"[{self.name}] 所有 API Key 均处于冷却中")
            return None

    def get_usable_keys(self) -> List[str]:
        """Return each non-cooling key once, in rotation order, for one attempt plan."""
        with self.key_state.lock:
            now = self._now()
            self._prune_cooldowns(now)
            num_keys = len(self.api_keys)
            if not num_keys:
                return []

            start = self.key_state.key_index
            usable: List[str] = []
            seen = set()
            for offset in range(num_keys):
                key = self.api_keys[(start + offset) % num_keys]
                if key in seen:
                    continue
                seen.add(key)
                if key not in self.key_state.key_cooldowns:
                    usable.append(key)
            if usable:
                self.key_state.key_index = (start + 1) % num_keys
            return usable

    def key_availability(self) -> Tuple[bool, List[float]]:
        """Return whether any key is usable plus the remaining positive cooldowns."""
        with self.key_state.lock:
            now = self._now()
            self._prune_cooldowns(now)
            if not self.api_keys:
                return False, []
            usable = any(k not in self.key_state.key_cooldowns for k in self.api_keys)
            remaining = [v - now for v in self.key_state.key_cooldowns.values() if v > now]
            return usable, remaining

    def is_key_cooling(self, key: str) -> bool:
        with self.key_state.lock:
            deadline = self.key_state.key_cooldowns.get(key)
            return deadline is not None and deadline > self._now()

    def mark_key_failed(self, key: str, cooldown_seconds: float = DEFAULT_HTTP_COOLDOWN_SECONDS):
        with self.key_state.lock:
            self.key_state.key_cooldowns[key] = self._now() + cooldown_seconds
            logger.warning(f"[{self.name}] Key ...{key[-6:] if len(key) > 6 else key} 失败，冷却 {cooldown_seconds}s")


class _Catalog:
    """One immutable catalog generation."""

    __slots__ = ("providers", "model_pools")

    def __init__(self, providers: Dict[str, ProviderState], model_pools: Dict[str, Dict[str, Any]]):
        self.providers = providers
        self.model_pools = model_pools


class GatewayEngine:
    def __init__(
        self,
        config_path: str,
        sync_opencode: bool = True,
        opencode_config_path: Optional[str] = None,
        clock: Optional[Callable[[], float]] = None,
    ):
        self.config_path = config_path
        self.sync_opencode = sync_opencode
        self.opencode_config_path = opencode_config_path
        self.clock = clock

        self._catalog_lock = threading.Lock()
        self._opencode_lock = threading.Lock()
        self._catalog = _Catalog({}, {})

        self.auth_require = True
        self.auth_token = DEFAULT_AUTH_TOKEN
        self.max_body_bytes = DEFAULT_MAX_BODY_BYTES
        self.max_cooldown_seconds = DEFAULT_MAX_COOLDOWN_SECONDS

        self.global_port = 17688
        self.auto_sync_interval = 3600
        self.running = False
        self.load_config()

    @property
    def providers(self) -> Dict[str, ProviderState]:
        return self._catalog.providers

    @property
    def model_pools(self) -> Dict[str, Dict[str, Any]]:
        return self._catalog.model_pools

    def _snapshot(self) -> Tuple[Dict[str, ProviderState], Dict[str, Dict[str, Any]]]:
        with self._catalog_lock:
            catalog = self._catalog
            return catalog.providers, catalog.model_pools

    def load_config(self):
        if not os.path.exists(self.config_path):
            logger.warning(f"配置文件未找到: {self.config_path}，使用空配置")
            return

        with open(self.config_path, "r", encoding="utf-8") as f:
            cfg = json.load(f)

        gw_cfg = cfg.get("ai_gateway", {})
        if not isinstance(gw_cfg, dict):
            gw_cfg = {}

        self.global_port = gw_cfg.get("port", 17688)
        self.auto_sync_interval = gw_cfg.get("auto_sync_interval_seconds", 3600)

        auth_cfg = gw_cfg.get("auth", {})
        if not isinstance(auth_cfg, dict):
            auth_cfg = {}
        self.auth_require = bool(auth_cfg.get("require", True))
        token = auth_cfg.get("token", DEFAULT_AUTH_TOKEN)
        self.auth_token = token if isinstance(token, str) and token else DEFAULT_AUTH_TOKEN

        max_body_bytes = gw_cfg.get("max_body_bytes", DEFAULT_MAX_BODY_BYTES)
        try:
            parsed_max_body = int(max_body_bytes)
        except (TypeError, ValueError):
            parsed_max_body = DEFAULT_MAX_BODY_BYTES
        self.max_body_bytes = parsed_max_body if parsed_max_body > 0 else DEFAULT_MAX_BODY_BYTES

        retry_cfg = gw_cfg.get("retry", {})
        if not isinstance(retry_cfg, dict):
            retry_cfg = {}
        raw_max_cooldown = retry_cfg.get("max_cooldown_seconds", DEFAULT_MAX_COOLDOWN_SECONDS)
        try:
            parsed_max_cooldown = float(raw_max_cooldown)
        except (TypeError, ValueError):
            parsed_max_cooldown = DEFAULT_MAX_COOLDOWN_SECONDS
        if not math.isfinite(parsed_max_cooldown) or parsed_max_cooldown <= 0:
            parsed_max_cooldown = DEFAULT_MAX_COOLDOWN_SECONDS
        self.max_cooldown_seconds = parsed_max_cooldown

        providers_data = gw_cfg.get("providers", {})
        if not isinstance(providers_data, dict):
            providers_data = {}
        new_providers: Dict[str, ProviderState] = {}
        for p_name, p_data in providers_data.items():
            if isinstance(p_data, dict):
                new_providers[p_name] = ProviderState(p_name, p_data, clock=self.clock)

        model_pools = gw_cfg.get("model_pools", {})
        if not isinstance(model_pools, dict):
            model_pools = {}

        with self._catalog_lock:
            self._catalog = _Catalog(new_providers, model_pools)

        logger.info(
            f"配置加载完成: {len(new_providers)} 个服务商, {len(model_pools)} 个模型池, 端口 {self.global_port}"
        )
        if self.sync_opencode:
            self.sync_to_opencode_config()

    @staticmethod
    def _upstream_model(provider: ProviderState, model_name: Any) -> Any:
        meta = provider.models.get(model_name)
        if isinstance(meta, dict):
            return meta.get("upstream_model", model_name)
        return model_name

    def resolve_model(self, requested_model: Any) -> Tuple[List[Tuple[str, str]], bool]:
        """Resolve a requested model into candidates plus whether the name is known.

        Only pools, active provider prefixes and explicit mappings on active
        providers resolve. Unknown names are never broadcast to providers.
        """
        name = requested_model if isinstance(requested_model, str) else ""
        if not name.strip():
            return [], False

        providers, pools = self._snapshot()

        # 1. 命中模型池 (Model Pool)
        if name in pools:
            pool = pools[name]
            candidates: List[Tuple[str, str]] = []
            if isinstance(pool, dict):
                for candidate in pool.get("candidates", []) or []:
                    if not isinstance(candidate, dict):
                        continue
                    p_name = candidate.get("provider")
                    m_name = candidate.get("model")
                    provider = providers.get(p_name)
                    if provider is not None and provider.is_active:
                        candidates.append((p_name, self._upstream_model(provider, m_name)))
            return candidates, True

        # 2. 命中具体服务商的前缀格式：provider/model（如 deepseek/deepseek-chat）
        if "/" in name:
            p_name, m_name = name.split("/", 1)
            if p_name and m_name:
                provider = providers.get(p_name)
                if provider is not None and provider.is_active:
                    return [(p_name, self._upstream_model(provider, m_name))], True

        # 3. 遍历各活跃服务商是否有直接对应的模型映射
        candidates = []
        known = False
        for p_name, provider in providers.items():
            if not provider.is_active:
                continue
            if name in provider.models:
                known = True
                candidates.append((p_name, self._upstream_model(provider, name)))

        return candidates, known

    def resolve_candidates(self, requested_model: str) -> List[Tuple[str, str]]:
        """
        将请求的模型名称解析为一个或多个候选 (provider_name, target_model) 列表。
        按优先级排序，以便于重试和降级。
        """
        return self.resolve_model(requested_model)[0]

    def sync_templates(self):
        """同步服务商模板中声明的在线模型"""
        logger.info("开始执行服务商模板模型自动同步...")
        providers, _ = self._snapshot()
        replacements: Dict[str, ProviderState] = {}

        for name, provider in providers.items():
            if not provider.template_url:
                continue
            try:
                req = urllib.request.Request(
                    provider.template_url,
                    headers={"User-Agent": "OneSpace-Cloud-Agent/1.0", "Accept": "application/json"},
                )
                with urllib.request.urlopen(req, timeout=15) as resp:
                    if resp.status == 200:
                        data = json.loads(resp.read().decode("utf-8"))
                        raw_models = data.get("data", []) or data.get("models", [])
                        new_models = dict(provider.models)
                        count = 0
                        for item in raw_models:
                            m_id = item.get("id") if isinstance(item, dict) else str(item)
                            if m_id and m_id not in new_models:
                                new_models[m_id] = {"upstream_model": m_id}
                                count += 1
                        replacements[name] = provider.with_models(new_models)
                        logger.info(f"[{name}] 模板同步成功，新增 {count} 个模型")
            except Exception as e:
                logger.warning(f"[{name}] 模板同步失败: {e}")

        if replacements:
            with self._catalog_lock:
                current = self._catalog
                new_providers = dict(current.providers)
                for name, provider in replacements.items():
                    if name in new_providers:
                        new_providers[name] = provider
                self._catalog = _Catalog(new_providers, current.model_pools)

        if self.sync_opencode:
            self.sync_to_opencode_config()

    def _build_opencode_models(
        self,
        providers: Dict[str, ProviderState],
        model_pools: Dict[str, Dict[str, Any]],
    ) -> Dict[str, Any]:
        models_dict: Dict[str, Any] = {}

        # 1. 优先放入虚拟模型池
        for pool_id, pool_meta in model_pools.items():
            models_dict[pool_id] = {
                "name": pool_meta.get("description", pool_id) if isinstance(pool_meta, dict) else pool_id,
                "limit": {"context": 1000000, "output": 0},
            }

        # 2. 放入各活跃服务商模型（保持纯净模型标识与显示名）
        for p_name, provider in providers.items():
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
                    "limit": {"context": ctx_window, "output": 0},
                }
                if reasoning_efforts:
                    model_entry["reasoning"] = True
                    model_entry["variants"] = {
                        rf: {"reasoningEffort": rf} for rf in reasoning_efforts
                    }

                models_dict[m_id] = model_entry

        return models_dict

    def sync_to_opencode_config(self, target_file: Optional[str] = None):
        """自动将当前网关可用的模型列表注入到 OpenCode 的主配置文件中，与 OneSpace App 保持完全一致"""
        target_path = target_file or self.opencode_config_path
        if not self.sync_opencode and not target_path:
            return

        if target_path:
            opencode_cfg_file = target_path
        else:
            cfg_dir = os.path.expanduser("~/.config/opencode")
            opencode_cfg_file = os.path.join(cfg_dir, "opencode.json")

        target_dir = os.path.dirname(os.path.abspath(opencode_cfg_file))
        jsonc_file = os.path.join(target_dir, "opencode.jsonc")

        # The whole read/merge/write transaction is serialized.
        with self._opencode_lock:
            try:
                os.makedirs(target_dir, exist_ok=True)

                if os.path.exists(jsonc_file):
                    logger.warning(
                        f"检测到 {jsonc_file}: OpenCode 可能优先使用 JSONC 配置，"
                        "请手动合并或删除该文件。"
                    )

                current_cfg: Dict[str, Any] = {}
                if os.path.exists(opencode_cfg_file):
                    try:
                        with open(opencode_cfg_file, "r", encoding="utf-8") as f:
                            parsed = json.load(f)
                    except Exception as e:
                        logger.warning(f"OpenCode 配置不是有效 JSON，已保持原样: {e}")
                        return
                    if not isinstance(parsed, dict):
                        logger.warning("OpenCode 配置不是 JSON 对象，已保持原样")
                        return
                    current_cfg = parsed

                providers, model_pools = self._snapshot()
                models_dict = self._build_opencode_models(providers, model_pools)

                if "$schema" not in current_cfg:
                    current_cfg["$schema"] = "https://opencode.ai/config.json"

                provider_cfg = current_cfg.get("provider")
                if not isinstance(provider_cfg, dict):
                    provider_cfg = {}

                # 与 OneSpace App 保持严格一致：只替换 gateway 提供商
                provider_cfg["gateway"] = {
                    "name": "Cloud Agent Gateway",
                    "npm": "@ai-sdk/openai-compatible",
                    "options": {
                        "baseURL": f"http://127.0.0.1:{self.global_port}/v1",
                        "apiKey": self.auth_token,
                    },
                    "models": models_dict,
                }
                current_cfg["provider"] = provider_cfg

                if "model" not in current_cfg and "cloud-coder" in model_pools:
                    current_cfg["model"] = "gateway/cloud-coder"

                fd, tmp_path = tempfile.mkstemp(
                    prefix=".opencode.", suffix=".tmp", dir=target_dir
                )
                try:
                    with os.fdopen(fd, "w", encoding="utf-8") as f:
                        json.dump(current_cfg, f, ensure_ascii=False, indent=2)
                        f.flush()
                        os.fsync(f.fileno())
                    os.replace(tmp_path, opencode_cfg_file)
                finally:
                    if os.path.exists(tmp_path):
                        try:
                            os.remove(tmp_path)
                        except OSError:
                            pass

                logger.info(
                    f"已按 OneSpace 规范同步 {len(models_dict)} 个模型到 OpenCode 终端配置 "
                    f"(单 Cloud Agent Gateway 提供商): {opencode_cfg_file}"
                )
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

    PROTECTED_GET_PATHS = ("/models", "/v1/models")
    HEALTH_PATHS = ("/health", "/v1/health")
    CHAT_PATHS = ("/chat/completions", "/v1/chat/completions")

    def log_message(self, format, *args):
        pass

    def send_json_response(self, status: int, data: Any, extra_headers: Optional[Dict[str, str]] = None):
        body = json.dumps(data, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        for header, value in (extra_headers or {}).items():
            self.send_header(header, value)
        self.end_headers()
        self.wfile.write(body)

    @staticmethod
    def _retry_after_seconds(headers: Any) -> Optional[float]:
        """Return a positive finite numeric Retry-After value, otherwise None."""
        if headers is None:
            return None
        raw = headers.get("Retry-After")
        if raw is None:
            return None
        try:
            value = float(str(raw).strip())
        except (TypeError, ValueError):
            return None
        if not math.isfinite(value) or value <= 0:
            return None
        return value

    def _http_failure_cooldown(self, headers: Any) -> float:
        retry_after = self._retry_after_seconds(headers)
        if retry_after is None:
            return DEFAULT_HTTP_COOLDOWN_SECONDS
        return min(retry_after, self.gateway.max_cooldown_seconds)

    def _send_all_cooling(self, providers: List["ProviderState"]) -> bool:
        """Send 429 when every configured key on the matching candidates is cooling."""
        keyed = [p for p in providers if p.api_keys]
        if not keyed:
            return False
        remaining: List[float] = []
        for provider in keyed:
            usable, cooldowns = provider.key_availability()
            if usable:
                return False
            remaining.extend(cooldowns)
        positive = [value for value in remaining if value > 0]
        if not positive:
            return False
        retry_after = max(1, int(math.ceil(min(positive))))
        self.send_json_response(
            429,
            {
                "error": {
                    "message": "所有上游 API Key 均在冷却中，请稍后重试。",
                    "type": "rate_limit_error",
                    "code": "all_keys_cooling",
                }
            },
            extra_headers={"Retry-After": str(retry_after)},
        )
        return True

    def _reject_origin(self) -> bool:
        """Universal Origin rejection before auth, body reading or forwarding."""
        if self.headers.get("Origin") is not None:
            self.send_json_response(
                403,
                {"error": {"message": "不允许来自浏览器 Origin 的请求", "type": "forbidden"}},
            )
            return True
        return False

    def _is_authorized(self) -> bool:
        if not self.gateway.auth_require:
            return True
        header = self.headers.get("Authorization")
        if not header:
            return False
        parts = header.split(None, 1)
        if len(parts) != 2 or parts[0].lower() != "bearer":
            return False
        return hmac.compare_digest(parts[1].strip(), self.gateway.auth_token)

    def _require_auth(self) -> bool:
        if self._is_authorized():
            return True
        self.send_json_response(
            401,
            {"error": {"message": "缺少或无效的 Bearer 凭据", "type": "authentication_error"}},
        )
        return False

    def do_OPTIONS(self):
        if self._reject_origin():
            return
        self.send_response(204)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_GET(self):
        if self._reject_origin():
            return

        parsed = urllib.parse.urlparse(self.path)
        path = parsed.path

        providers, model_pools = self.gateway._snapshot()

        if path in self.HEALTH_PATHS:
            self.send_json_response(200, {
                "status": "ok",
                "providers_count": len(providers),
                "model_pools": list(model_pools.keys()),
            })
            return

        if path in self.PROTECTED_GET_PATHS:
            if not self._require_auth():
                return
            models_list = []
            for pool_id, pool_meta in model_pools.items():
                models_list.append({
                    "id": pool_id,
                    "object": "model",
                    "created": int(time.time()),
                    "owned_by": "onespace-model-pool",
                    "description": pool_meta.get("description", "Virtual Model Pool")
                    if isinstance(pool_meta, dict) else "Virtual Model Pool",
                })
            for p_name, provider in providers.items():
                for m_id in provider.models.keys():
                    models_list.append({
                        "id": f"{p_name}/{m_id}",
                        "object": "model",
                        "created": int(time.time()),
                        "owned_by": p_name,
                    })

            self.send_json_response(200, {"object": "list", "data": models_list})
            return

        self.send_json_response(404, {"error": {"message": f"未找到端点: {path}"}})

    def do_POST(self):
        if self._reject_origin():
            return

        parsed = urllib.parse.urlparse(self.path)
        path = parsed.path

        if path not in self.CHAT_PATHS:
            self.send_json_response(404, {"error": {"message": f"不支持的端点: {path}"}})
            return

        if not self._require_auth():
            return

        raw_length = self.headers.get("Content-Length")
        if raw_length is None:
            self.send_json_response(400, {"error": {"message": "缺少 Content-Length"}})
            return

        value = raw_length.strip()
        if not value.isascii() or not value.isdigit():
            self.send_json_response(400, {"error": {"message": "Content-Length 无效"}})
            return

        content_length = int(value)
        if content_length <= 0:
            self.send_json_response(400, {"error": {"message": "请求体为空"}})
            return
        if content_length > self.gateway.max_body_bytes:
            self.send_json_response(413, {"error": {"message": "请求体超出大小限制"}})
            return

        try:
            req_body_bytes = self.rfile.read(content_length)
            req_data = json.loads(req_body_bytes.decode("utf-8"))
        except Exception as e:
            self.send_json_response(400, {"error": {"message": f"请求 JSON 解析失败: {e}"}})
            return

        if not isinstance(req_data, dict):
            self.send_json_response(400, {"error": {"message": "请求 JSON 必须是对象"}})
            return

        requested_model = req_data.get("model", "")
        candidates, known = self.gateway.resolve_model(requested_model)

        if not candidates:
            if known:
                self.send_json_response(502, {
                    "error": {
                        "message": f"模型 '{requested_model}' 已知但当前没有可用候选。",
                        "type": "operational_unavailability",
                    }
                })
            else:
                self.send_json_response(404, {
                    "error": {
                        "message": f"未找到模型 '{requested_model}'。",
                        "type": "model_not_found",
                        "code": "model_not_found",
                    }
                })
            return

        is_stream = req_data.get("stream", False) is True

        providers = self.gateway.providers
        active_providers: List[ProviderState] = []
        for p_name, _ in candidates:
            provider = providers.get(p_name)
            if provider is not None and provider.is_active:
                active_providers.append(provider)

        # A fully cooling candidate set is answered without any upstream attempt.
        if active_providers and self._send_all_cooling(active_providers):
            return

        last_error_message = "未知错误"
        for idx, (p_name, target_model) in enumerate(candidates):
            provider = providers.get(p_name)
            if not provider or not provider.is_active:
                continue

            api_keys = provider.get_usable_keys()
            if not api_keys:
                if not provider.api_keys:
                    logger.warning(f"[{p_name}] 无可用 API Key，跳过")
                else:
                    logger.warning(f"[{p_name}] 所有 API Key 均处于冷却中，跳过")
                continue

            req_data["model"] = target_model
            forward_body = json.dumps(req_data, ensure_ascii=False).encode("utf-8")

            upstream_url = f"{provider.base_url}/chat/completions"
            if not upstream_url.startswith("http"):
                upstream_url = "https://" + upstream_url

            logger.info(
                f"请求转发: model={requested_model} -> [{p_name}] model={target_model} "
                f"(候选 {idx+1}/{len(candidates)}, {len(api_keys)} 个可用 Key)"
            )

            for api_key in api_keys:
                # Another request may have cooled this key since the snapshot.
                if provider.is_key_cooling(api_key):
                    continue

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
                except urllib.error.HTTPError as e:
                    err_text = ""
                    try:
                        err_text = e.read().decode("utf-8")
                    except Exception:
                        pass
                    last_error_message = f"[{p_name}] HTTP {e.code}: {err_text or e.reason}"
                    logger.warning(f"转发失败: {last_error_message}")

                    if e.code == 429 or 500 <= e.code < 600:
                        provider.mark_key_failed(
                            api_key, self._http_failure_cooldown(e.headers)
                        )
                        continue

                    # Non-retryable business errors fail fast without failover.
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
                    provider.mark_key_failed(
                        api_key, cooldown_seconds=DEFAULT_TRANSPORT_COOLDOWN_SECONDS
                    )
                    continue

                if is_stream:
                    self._relay_stream(resp)
                    return

                resp_body = resp.read()
                self.send_response(resp.status)
                self.send_header("Content-Type", "application/json; charset=utf-8")
                self.send_header("Content-Length", str(len(resp_body)))
                self.end_headers()
                self.wfile.write(resp_body)
                return

        if active_providers and self._send_all_cooling(active_providers):
            return

        self.send_json_response(502, {
            "error": {
                "message": f"所有上游候选均重试失败。最后错误: {last_error_message}"
            }
        })

    def _relay_stream(self, resp: Any) -> None:
        """Relay upstream bytes promptly and keep downstream/upstream faults distinct."""
        committed = False
        try:
            self.send_response(resp.status)
            for k, v in resp.getheaders():
                if k.lower() in ("content-type", "cache-control", "connection"):
                    self.send_header(k, v)
            self.end_headers()
            committed = True

            read1 = getattr(resp, "read1", None)
            while True:
                try:
                    if read1 is not None:
                        chunk = read1(STREAM_READ_SIZE)
                    else:
                        chunk = resp.read(STREAM_READ_SIZE)
                except Exception as e:
                    if committed:
                        logger.warning(f"上游流式响应头已提交后读取失败，关闭连接: {e}")
                    else:
                        logger.warning(f"上游流式读取失败: {e}")
                    self.close_connection = True
                    return
                if not chunk:
                    break
                try:
                    self.wfile.write(chunk)
                    self.wfile.flush()
                except (BrokenPipeError, ConnectionResetError, OSError) as e:
                    logger.info(f"下游客户端主动断开连接: {e}")
                    self.close_connection = True
                    return
        except (BrokenPipeError, ConnectionResetError, OSError) as e:
            logger.info(f"下游客户端在响应头阶段断开连接: {e}")
            self.close_connection = True
        finally:
            try:
                resp.close()
            except Exception:
                pass


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
