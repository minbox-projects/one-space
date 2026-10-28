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
from core.issue_worker import IssueWorker, start_worker_loop
from core.notifier import Notifier

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] [%(name)s] %(message)s"
)
logger = logging.getLogger("onespace-agent")


class CloudAgentDaemon:
    def __init__(self, config_path: str):
        self.config_path = os.path.abspath(config_path)
        if not os.path.exists(self.config_path):
            logger.error(f"配置文件不存在: {self.config_path}")
            sys.exit(1)

        with open(self.config_path, "r", encoding="utf-8") as f:
            self.config = json.load(f)

        self.guardian = DiskGuardian(min_free_gb=2.0)
        self.notifier = Notifier(self.config)
        self.gateway = GatewayEngine(self.config_path)
        self.worker = IssueWorker(self.config, self.guardian, self.notifier)
        
        self.httpd = None
        self.running = True

    def start_gateway(self):
        self.gateway.running = True
        self.gateway.start_background_tasks()

        class BoundHandler(GatewayRequestHandler):
            pass
        BoundHandler.gateway = self.gateway

        port = self.gateway.global_port
        server_address = ("127.0.0.1", port)
        self.httpd = http.server.ThreadingHTTPServer(server_address, BoundHandler)
        logger.info(f"Linux AI 网关在后台启动: http://127.0.0.1:{port}")
        self.httpd.serve_forever()

    def run(self):
        logger.info("==================================================")
        logger.info("       OneSpace Cloud Agent 守护进程启动中        ")
        logger.info("==================================================")

        self.guardian.run_routine_check()

        gateway_thread = threading.Thread(target=self.start_gateway, daemon=True, name="agent-gateway")
        gateway_thread.start()

        worker_thread = threading.Thread(target=start_worker_loop, args=(self.worker,), daemon=True, name="agent-worker")
        worker_thread.start()

        def _signal_handler(signum, frame):
            logger.info(f"接收到信号 {signum}，正在平滑退出守护进程...")
            self.running = False
            if self.httpd:
                self.httpd.shutdown()
            sys.exit(0)

        signal.signal(signal.SIGINT, _signal_handler)
        signal.signal(signal.SIGTERM, _signal_handler)

        logger.info("OneSpace Cloud Agent 运行中，按 Ctrl+C 退出。")
        while self.running:
            time.sleep(1)


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
        # 非 systemd 环境或本地测试
        service_active = True

    return {
        "service_active": service_active,
        "disk": disk_info,
        "gateway": gateway_health,
        "timestamp": int(time.time())
    }


if __name__ == "__main__":
    args = sys.argv[1:]
    is_json = "--json" in args

    if args and args[0] == "status":
        cfg = "config.json"
        for a in args[1:]:
            if not a.startswith("-"):
                cfg = a
                break
        report = get_status_report(cfg)
        if is_json:
            print(json.dumps(report, ensure_ascii=False, indent=2))
        else:
            print(f"Service Active: {report['service_active']}")
            print(f"Disk: {report['disk']['used_gb']}G / {report['disk']['total_gb']}G ({report['disk']['use_percent']}%)")
            print(f"Gateway: {report['gateway'].get('status', 'unknown')}")
        sys.exit(0)

    if args and args[0] == "tasks":
        tasks = IssueWorker.read_task_history_file()
        if is_json:
            print(json.dumps(tasks, ensure_ascii=False, indent=2))
        else:
            print(f"Total Tasks: {len(tasks)}")
            for t in tasks:
                print(f"  - [{t.get('status')}] {t.get('repo')}#{t.get('issue_number')} {t.get('title')}")
        sys.exit(0)

    cfg_file = args[0] if args and not args[0].startswith("-") else "config.json"
    daemon = CloudAgentDaemon(cfg_file)
    daemon.run()
