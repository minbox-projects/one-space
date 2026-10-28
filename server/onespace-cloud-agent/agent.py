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
import sys
import threading
import time
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


if __name__ == "__main__":
    cfg_file = sys.argv[1] if len(sys.argv) > 1 else "config.json"
    daemon = CloudAgentDaemon(cfg_file)
    daemon.run()
