#!/usr/bin/env python3
"""
onespace-cloud-agent: 主守护进程入口 (Daemon Entrypoint)
同时管控 Linux AI 网关、GitHub Issue 自动 PR 监听器与磁盘守护者。
"""

import http.server
import json
import logging
import os
import signal
import subprocess
import sys
import threading
import time
import urllib.request
from core.disk_guardian import DiskGuardian
from core.gateway import GatewayEngine, GatewayRequestHandler
from core.issue_worker import IssueWorker
from core.notifier import Notifier
from core.plan_tasks import PlanError, PlanTaskManager, SCHEMA_VERSION

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] [%(name)s] %(message)s"
)
logger = logging.getLogger("onespace-agent")

DEFAULT_DISK_CHECK_INTERVAL_SECONDS = 300
DEFAULT_SHUTDOWN_GRACE_SECONDS = 300


class CloudAgentDaemon:
    """Supervise the gateway, one Issue worker and periodic maintenance.

    A signal only records stop state and a shared monotonic shutdown deadline;
    the gateway remains available to the in-flight task until that deadline.
    """

    def __init__(
        self,
        config_path: str,
        *,
        gateway=None,
        gateway_factory=None,
        worker=None,
        guardian=None,
        notifier=None,
        clock=time.monotonic,
        sleep=time.sleep,
        register_signal=None,
        exit_process=None,
    ):
        self.config_path = os.path.abspath(config_path)
        if not os.path.exists(self.config_path):
            logger.error(f"配置文件不存在: {self.config_path}")
            raise SystemExit(1)

        with open(self.config_path, "r", encoding="utf-8") as f:
            self.config = json.load(f)

        self._clock = clock or time.monotonic
        self._sleep = sleep or time.sleep
        self._register_signal = register_signal
        self._exit_process = exit_process

        runtime_cfg = self.config.get("runtime", {})
        if not isinstance(runtime_cfg, dict):
            runtime_cfg = {}
        self.disk_check_interval = float(
            runtime_cfg.get(
                "disk_check_interval_seconds",
                DEFAULT_DISK_CHECK_INTERVAL_SECONDS,
            )
        )
        self.shutdown_grace = float(
            runtime_cfg.get(
                "shutdown_grace_seconds",
                DEFAULT_SHUTDOWN_GRACE_SECONDS,
            )
        )
        self._next_maintenance = self._clock() + self.disk_check_interval

        self.running = True
        self.shutdown_deadline = None
        self._shutdown_started_real = None
        self.exit_code = None
        self.httpd = None
        self._stop_event = threading.Event()
        self._deadline_handled = False
        self._gateway_thread = None
        self._worker_thread = None

        worker_cfg = self.config.get("issue_worker", {})
        if not isinstance(worker_cfg, dict):
            worker_cfg = {}
        try:
            self.poll_interval = max(
                1.0, float(worker_cfg.get("poll_interval_seconds", 180))
            )
        except (TypeError, ValueError):
            self.poll_interval = 180.0

        guardian_cfg = self.config.get("guardian", {})
        if not isinstance(guardian_cfg, dict):
            guardian_cfg = {}
        self.guardian = guardian or DiskGuardian(
            min_free_gb=float(guardian_cfg.get("min_free_gb", 2.0)),
            data_roots=guardian_cfg.get("data_roots"),
            cache_retention_days=int(guardian_cfg.get("cache_retention_days", 7)),
            cleanup_sessions=bool(guardian_cfg.get("cleanup_sessions", False)),
            worktree_retention_days=int(
                guardian_cfg.get("worktree_retention_days", 7)
            ),
            workspace_root=guardian_cfg.get(
                "workspace_root",
                worker_cfg.get("workspace_root", "/root/cloud-agent-workspaces"),
            ),
        )
        self.notifier = notifier or Notifier(
            self.config,
            sleep=self._sleep,
            clock=self._clock,
            stop_event=self._stop_event,
            deadline=self._shutdown_deadline_value,
        )

        if gateway is not None:
            self.gateway = gateway
            self._owns_gateway = False
        elif gateway_factory is not None:
            self.gateway = gateway_factory()
            self._owns_gateway = True
        else:
            self.gateway = GatewayEngine(self.config_path)
            self._owns_gateway = True

        self.worker = worker or IssueWorker(
            self.config,
            self.guardian,
            self.notifier,
            clock=self._clock,
            sleep=self._sleep,
            stop_event=self._stop_event,
            deadline=self._shutdown_deadline_value,
        )

        if register_signal is not None:
            self._install_signal_handlers()

    def _shutdown_deadline_value(self):
        return self.shutdown_deadline

    def _install_signal_handlers(self):
        registrar = self._register_signal or signal.signal
        registrar(signal.SIGINT, self._handle_signal)
        registrar(signal.SIGTERM, self._handle_signal)

    def _handle_signal(self, signum, frame):
        """Record stop state and one shared deadline; never block or exit."""
        logger.info(f"接收到信号 {signum}，进入有限宽限关闭流程...")
        if self.shutdown_deadline is None:
            self.shutdown_deadline = self._clock() + self.shutdown_grace
            self._shutdown_started_real = time.monotonic()
        self.running = False
        self._stop_event.set()

    def run_maintenance(self):
        """Run one disk guardian check, independent of the worker."""
        try:
            self.guardian.run_routine_check()
        except Exception as e:  # noqa: BLE001 - maintenance must not stop the daemon
            logger.error(f"磁盘巡检失败: {e}")

    def _deadline_reached(self) -> bool:
        return (
            self.shutdown_deadline is not None
            and self._clock() >= self.shutdown_deadline
        )

    def _handle_deadline_expiry(self):
        """Terminate only the owned Issue process group and record one terminal."""
        if self._deadline_handled:
            return
        self._deadline_handled = True
        logger.error("关闭宽限期已到，终止当前任务自有子进程组...")
        raw_attempts = getattr(self.worker, "current_attempts", 0)
        try:
            attempts = int(raw_attempts or 0)
        except (TypeError, ValueError):
            attempts = 0
        try:
            self.worker.record_terminal_shutdown(attempts)
        except Exception as e:  # noqa: BLE001 - shutdown must stay bounded
            logger.error(f"写入关闭终态失败: {e}")
        try:
            self.worker.terminate_active_process_group()
        except Exception as e:  # noqa: BLE001
            logger.error(f"终止任务子进程组失败: {e}")
        self.running = False
        self._stop_event.set()

    def _on_gateway_exit(self):
        """Unexpected gateway exit requests a nonzero owned exit."""
        if self.exit_code is None:
            self.exit_code = -1
        logger.error("AI 网关线程意外退出，请求以非零状态退出。")
        self.running = False
        self._stop_event.set()
        if self._exit_process is not None:
            try:
                self._exit_process(-1)
            except Exception as e:  # noqa: BLE001
                logger.error(f"退出回调失败: {e}")

    def _serve_gateway(self):
        try:
            self.gateway.running = True
            start_tasks = getattr(self.gateway, "start_background_tasks", None)
            if callable(start_tasks):
                start_tasks()

            class BoundHandler(GatewayRequestHandler):
                pass

            BoundHandler.gateway = self.gateway
            port = self.gateway.global_port
            server_address = ("127.0.0.1", port)
            self.httpd = http.server.ThreadingHTTPServer(
                server_address, BoundHandler
            )
            logger.info(f"Linux AI 网关在后台启动: http://127.0.0.1:{port}")
            self.httpd.serve_forever(poll_interval=0.2)
        except Exception as e:  # noqa: BLE001 - surfaced by the supervisor
            logger.error(f"AI 网关线程异常退出: {e}")
        finally:
            self.gateway.running = False

    def _worker_loop(self):
        while self.running and not self._stop_event.is_set():
            try:
                self.worker.run_one_issue()
            except Exception as e:  # noqa: BLE001 - worker loop must survive
                logger.error(f"Issue 工作循环发生未捕获异常: {e}")
            if not self.running or self._deadline_reached():
                break
            if self._sleep is time.sleep:
                self._stop_event.wait(self.poll_interval)
            else:
                self._sleep(self.poll_interval)

    def _wait_for_worker(self):
        thread = self._worker_thread
        if thread is None:
            return
        if self.shutdown_deadline is None:
            thread.join(timeout=1.0)
            return
        real_deadline = None
        if self._shutdown_started_real is not None:
            real_deadline = self._shutdown_started_real + self.shutdown_grace
        while thread.is_alive():
            remaining = self.shutdown_deadline - self._clock()
            if remaining <= 0:
                self._handle_deadline_expiry()
                thread.join(timeout=1.0)
                return
            if real_deadline is not None and time.monotonic() >= real_deadline:
                self._handle_deadline_expiry()
                thread.join(timeout=1.0)
                return
            thread.join(timeout=min(1.0, max(0.01, remaining)))
            self.run_maintenance()

    def run(self):
        logger.info("==================================================")
        logger.info("       OneSpace Cloud Agent 守护进程启动中        ")
        logger.info("==================================================")

        self.running = True
        self._deadline_handled = False
        self._install_signal_handlers()
        self.run_maintenance()

        if self._owns_gateway:
            self._gateway_thread = threading.Thread(
                target=self._serve_gateway, daemon=True, name="agent-gateway"
            )
            self._gateway_thread.start()

        self._worker_thread = threading.Thread(
            target=self._worker_loop, daemon=True, name="agent-worker"
        )
        self._worker_thread.start()

        logger.info("OneSpace Cloud Agent 运行中，按 Ctrl+C 退出。")
        try:
            while self.running:
                now = self._clock()
                if now >= self._next_maintenance:
                    self.run_maintenance()
                    self._next_maintenance = now + self.disk_check_interval
                if self._deadline_reached():
                    self._handle_deadline_expiry()
                    break
                if (
                    self._gateway_thread is not None
                    and not self._gateway_thread.is_alive()
                ):
                    self._on_gateway_exit()
                    break
                self._sleep(1.0)
        finally:
            self.running = False
            self._stop_event.set()
            if self.shutdown_deadline is not None:
                self._wait_for_worker()
            if self.httpd is not None:
                try:
                    self.httpd.shutdown()
                except Exception:  # noqa: BLE001
                    pass
                try:
                    self.httpd.server_close()
                except Exception:  # noqa: BLE001
                    pass
            try:
                self.gateway.running = False
            except Exception:  # noqa: BLE001
                pass

        if self.exit_code is not None:
            return self.exit_code
        return 0


def get_status_report(config_path: str = "config.json") -> dict:
    """生成系统健康与服务状态报表"""
    guardian = DiskGuardian()
    disk_info = guardian.get_disk_usage("/")

    # 检查网关连通性
    port = 17688
    if os.path.exists(config_path):
        try:
            with open(config_path, "r", encoding="utf-8") as f:
                cfg = json.load(f)
                port = cfg.get("ai_gateway", {}).get("port", 17688)
        except Exception:
            pass

    gateway_health = {"status": "unreachable", "port": port}
    try:
        req = urllib.request.Request(f"http://127.0.0.1:{port}/health")
        with urllib.request.urlopen(req, timeout=2) as resp:
            if resp.status == 200:
                gateway_health = json.loads(resp.read().decode("utf-8"))
    except Exception as e:
        gateway_health["error"] = str(e)

    # 若未从实时网关拿到 providers（例如启动中或离线），尝试从 config.json 读取静态配置回退
    if "providers" not in gateway_health and os.path.exists(config_path):
        try:
            with open(config_path, "r", encoding="utf-8") as f:
                cfg = json.load(f)
                gw_section = cfg.get("ai_gateway", {})
                cfg_providers = gw_section.get("providers", {})
                cfg_pools = list(gw_section.get("model_pools", {}).keys())
                fallback_providers = []
                for p_id, p_info in cfg_providers.items():
                    if not isinstance(p_info, dict):
                        continue
                    m_list = []
                    for m_id, m_meta in p_info.get("models", {}).items():
                        m_dict = {
                            "id": m_id,
                            "upstream_model": m_id,
                            "display_name": m_id,
                            "context_window": None,
                            "reasoning_efforts": [],
                        }
                        if isinstance(m_meta, dict):
                            m_dict["upstream_model"] = m_meta.get("upstream_model", m_id)
                            m_dict["display_name"] = m_meta.get("display_name", m_id)
                            m_dict["context_window"] = m_meta.get("context_window")
                            m_dict["reasoning_efforts"] = m_meta.get("reasoning_efforts", []) or []
                        m_list.append(m_dict)
                    fallback_providers.append({
                        "id": p_id,
                        "name": p_info.get("name", p_id) or p_id,
                        "base_url": p_info.get("base_url", ""),
                        "is_active": p_info.get("is_active", True),
                        "models": m_list,
                    })
                if fallback_providers:
                    gateway_health["providers"] = fallback_providers
                    if "providers_count" not in gateway_health:
                        gateway_health["providers_count"] = len(fallback_providers)
                if cfg_pools and "model_pools" not in gateway_health:
                    gateway_health["model_pools"] = cfg_pools
        except Exception:
            pass

    # 检查 systemd 服务状态
    service_active = False
    try:
        res = subprocess.run(
            ["systemctl", "is-active", "onespace-cloud-agent"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False
        )
        service_active = (res.stdout.strip() == "active")
    except Exception:
        # 无法确认 systemd 服务状态时不得报告为活动
        service_active = False

    return {
        "service_active": service_active,
        "disk": disk_info,
        "gateway": gateway_health,
        "timestamp": int(time.time())
    }


PLAN_CAPABILITIES = {
    "plan_submit": True,
    "plan_list": True,
    "plan_cancel": True,
    "plan_attach": True,
}


def _extract_config(args):
    """Remove ``--config <path>`` / ``--config=<path>`` and return the rest."""
    remaining = []
    config_path = None
    index = 0
    while index < len(args):
        value = args[index]
        if value == "--config":
            if index + 1 < len(args):
                config_path = args[index + 1]
                index += 2
                continue
            index += 1
            continue
        if value.startswith("--config="):
            config_path = value.split("=", 1)[1]
            index += 1
            continue
        remaining.append(value)
        index += 1
    return remaining, config_path


def _load_config(config_path):
    if config_path and os.path.exists(config_path):
        try:
            with open(config_path, "r", encoding="utf-8") as handle:
                data = json.load(handle)
            if isinstance(data, dict):
                return data
        except Exception as exc:  # noqa: BLE001 - defaults remain usable
            logger.warning("读取配置失败 %s: %s", config_path, exc)
    return {}


def _emit_json(payload, stdout):
    stdout.write(json.dumps(payload, ensure_ascii=False, indent=2) + "\n")
    try:
        stdout.flush()
    except Exception:  # noqa: BLE001
        pass


def _read_stdin_bytes(stdin):
    buffer = getattr(stdin, "buffer", None)
    data = buffer.read() if buffer is not None else stdin.read()
    if isinstance(data, str):
        data = data.encode("utf-8")
    return data


def _handle_plan(args, stdin, stdout):
    remaining, config_arg = _extract_config(args)
    subcommand = remaining[0] if remaining else ""
    positional = [value for value in remaining[1:] if not value.startswith("-")]
    config_path = config_arg or (
        "config.json" if os.path.exists("config.json") else None
    )
    manager = PlanTaskManager(_load_config(config_path), config_path=config_path)

    if subcommand == "capabilities":
        result = {
            "ok": True,
            "schema_version": SCHEMA_VERSION,
            "capabilities": dict(PLAN_CAPABILITIES),
        }
    elif subcommand == "submit":
        result = manager.submit(_read_stdin_bytes(stdin), None)
    elif subcommand == "list":
        result = {"ok": True, "tasks": manager.list_tasks()}
    elif subcommand == "inspect":
        job_id = positional[0] if positional else None
        task = manager.inspect(job_id) if job_id else None
        if task is None:
            result = {"ok": False, "code": "not_found", "error": "unknown plan job"}
        else:
            result = {"ok": True, "task": task}
    elif subcommand == "cancel":
        if positional:
            result = manager.cancel(positional[0])
        else:
            result = {"ok": False, "code": "not_found", "error": "unknown plan job"}
    elif subcommand == "run":
        if not positional:
            result = {"ok": False, "code": "not_found", "error": "unknown plan job"}
        else:
            try:
                result = {"ok": True, "task": manager.run_job(positional[0])}
            except PlanError as exc:
                result = {"ok": False, "code": exc.code, "error": exc.message}
            except Exception as exc:  # noqa: BLE001 - only internal errors are nonzero
                logger.exception("plan run failed")
                result = {"ok": False, "code": "internal_error", "error": str(exc)}
                _emit_json(result, stdout)
                return 1
    else:
        result = {
            "ok": False,
            "code": "unknown_command",
            "error": "unknown plan command",
        }

    _emit_json(result, stdout)
    return 0


def _collect_tasks(config_path):
    tasks = IssueWorker.read_task_history_file()
    try:
        manager = PlanTaskManager(_load_config(config_path), config_path=config_path)
        tasks = manager.aggregate_tasks(tasks)
    except Exception as exc:  # noqa: BLE001 - Issue history remains available
        logger.warning("读取 Plan 任务失败: %s", exc)
    return tasks


def main(argv=None, stdin=None, stdout=None):
    """Run one CLI command without starting the daemon for command families."""
    args = list(sys.argv[1:] if argv is None else argv)
    stdin = sys.stdin if stdin is None else stdin
    stdout = sys.stdout if stdout is None else stdout
    is_json = "--json" in args

    if args and args[0] == "status":
        remaining, config_arg = _extract_config(args[1:])
        cfg = config_arg or next(
            (value for value in remaining if not value.startswith("-")),
            "config.json",
        )
        report = get_status_report(cfg)
        if is_json:
            stdout.write(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
        else:
            stdout.write(f"Service Active: {report['service_active']}\n")
            stdout.write(
                f"Disk: {report['disk']['used_gb']}G / "
                f"{report['disk']['total_gb']}G "
                f"({report['disk']['use_percent']}%)\n"
            )
            stdout.write(f"Gateway: {report['gateway'].get('status', 'unknown')}\n")
        return 0

    if args and args[0] == "tasks":
        remaining, config_arg = _extract_config(args[1:])
        config_path = config_arg or (
            "config.json" if os.path.exists("config.json") else None
        )
        tasks = _collect_tasks(config_path)
        if is_json:
            stdout.write(json.dumps(tasks, ensure_ascii=False, indent=2) + "\n")
        else:
            stdout.write(f"Total Tasks: {len(tasks)}\n")
            for task in tasks:
                if task.get("issue_number") is None:
                    reference = f"[{task.get('source', 'plan')}]"
                else:
                    reference = f"#{task.get('issue_number')}"
                stdout.write(
                    f"  - [{task.get('status')}] {task.get('repo')}"
                    f"{reference} {task.get('title')}\n"
                )
        return 0

    if args and args[0] == "plan":
        return _handle_plan(args[1:], stdin, stdout)

    remaining, config_arg = _extract_config(args)
    if config_arg:
        cfg_file = config_arg
    elif remaining and not remaining[0].startswith("-"):
        cfg_file = remaining[0]
    else:
        cfg_file = "config.json"
    daemon = CloudAgentDaemon(cfg_file)
    return daemon.run()


if __name__ == "__main__":
    sys.exit(main())
