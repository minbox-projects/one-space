#!/usr/bin/env python3
"""Tests for CloudAgentDaemon lifecycle (AC-030).

Documented injectable public surface (implementation must satisfy):

CloudAgentDaemon(
    config_path,
    *,
    gateway=None,            # injected gateway object
    gateway_factory=None,    # callable returning a gateway object
    worker,                 # IssueWorker-compatible
    guardian=None,          # DiskGuardian-compatible
    notifier=None,
    clock=time.monotonic,   # monotonic clock callable
    sleep=time.sleep,       # sleep callable
    register_signal=None,   # callable(signum, handler)
    exit_process=None,      # callable(exit_code)
)

Attributes: .gateway, .worker, .guardian, .notifier, .running,
           .shutdown_deadline (None or float), .exit_code (int or None).
Methods: .run() (blocks; returns exit code), ._handle_signal(signum, frame)
         (records stop/deadline only), .run_maintenance() (one disk check).
"""

import json
import logging
import os
import signal
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest import mock

from agent import CloudAgentDaemon


def _write_config(tmp, disk_interval=300, grace=300):
    cfg = {
        "ai_gateway": {"port": 17699},
        "runtime": {
            "disk_check_interval_seconds": disk_interval,
            "shutdown_grace_seconds": grace,
        },
    }
    path = os.path.join(tmp, "config.json")
    with open(path, "w", encoding="utf-8") as f:
        json.dump(cfg, f)
    return path


class FakeClock:
    def __init__(self, start=0.0):
        self.now = start
        self.lock = threading.Lock()

    def __call__(self):
        with self.lock:
            return self.now

    def advance(self, delta):
        with self.lock:
            self.now += delta


class TestAgentLifecycle(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="test_agent_")
        self.config_path = _write_config(self.tmp)
        self.clock = FakeClock()
        self.sleeps = []
        self.exit_codes = []
        self.signals = {}
        self.maintenance_calls = []

        self.guardian = mock.Mock()
        self.guardian.run_routine_check.side_effect = lambda: self.maintenance_calls.append(self.clock())

        self.worker = mock.Mock()
        self.worker.running = True
        self.notifier = mock.Mock()
        self.gateway = mock.Mock()
        self.gateway.running = True
        self.gateway.global_port = 17699

        def register_signal(signum, handler):
            self.signals[signum] = handler

        self.daemon = CloudAgentDaemon(
            self.config_path,
            gateway=self.gateway,
            worker=self.worker,
            guardian=self.guardian,
            notifier=self.notifier,
            clock=self.clock,
            sleep=lambda s: self.sleeps.append(s),
            register_signal=register_signal,
            exit_process=lambda code: self.exit_codes.append(code),
        )

    def tearDown(self):
        import shutil
        shutil.rmtree(self.tmp, ignore_errors=True)

    # 1. periodic maintenance
    def test_periodic_maintenance_runs_independently(self):
        barrier = threading.Event()
        self.worker.run_one_issue.side_effect = lambda: barrier.wait(timeout=2)
        t = threading.Thread(target=self.daemon.run, daemon=True)
        t.start()
        self.clock.advance(301)
        time.sleep(0.2)
        barrier.set()
        t.join(timeout=2)
        self.assertGreaterEqual(self.guardian.run_routine_check.call_count, 2)
        self.worker.run_one_issue.assert_called()

    # 2. gateway supervision (legacy hook)
    def test_gateway_thread_exit_requests_nonzero_exit(self):
        self.gateway.running = False
        if hasattr(self.daemon, "_on_gateway_exit"):
            self.daemon._on_gateway_exit()
        self.assertTrue(self.exit_codes and self.exit_codes[-1] != 0)

    # 3. signal handling
    def test_sigterm_handler_only_records_state(self):
        handler = self.signals.get(signal.SIGTERM)
        self.assertIsNotNone(handler)
        before = time.time()
        handler(signal.SIGTERM, None)
        self.assertLess(time.time() - before, 1.0)
        self.assertFalse(self.daemon.running)
        self.assertIsNotNone(self.daemon.shutdown_deadline)
        calls_before = self.worker.run_one_issue.call_count
        time.sleep(0.1)
        calls_after = self.worker.run_one_issue.call_count
        self.assertEqual(calls_before, calls_after)

    # 4. deadline expiry (legacy hook)
    def test_deadline_expiry_terminates_owned_group_and_writes_terminal(self):
        self.daemon.shutdown_deadline = self.clock()
        if hasattr(self.daemon, "_handle_deadline_expiry"):
            self.daemon._handle_deadline_expiry()
        self.worker.record_terminal_shutdown.assert_called() if hasattr(self.worker, "record_terminal_shutdown") else True
        self.assertFalse(hasattr(self.guardian, "kill_session") and self.guardian.kill_session.called)


class TestAgentRunLevel(unittest.TestCase):
    def _sleep_factory(self, sleeps):
        def _sleep(s):
            sleeps.append(s)
            time.sleep(0.001)
        return _sleep

    def test_gateway_thread_death_through_run_exits_nonzero(self):
        tmp = tempfile.mkdtemp(prefix="test_agent_")
        try:
            config_path = _write_config(tmp)
            clock = FakeClock()
            sleeps = []
            exit_codes = []
            signals = {}
            guardian = mock.Mock()
            worker = mock.Mock()
            worker.running = True

            class FakeGateway:
                def __init__(self):
                    self.running = False
                    self.global_port = 17699

                def start_background_tasks(self):
                    raise RuntimeError("gateway boom")

            factory_calls = []

            def factory():
                g = FakeGateway()
                factory_calls.append(g)
                return g

            daemon = CloudAgentDaemon(
                config_path,
                gateway_factory=factory,
                worker=worker,
                guardian=guardian,
                notifier=mock.Mock(),
                clock=clock,
                sleep=self._sleep_factory(sleeps),
                register_signal=lambda signum, handler: signals.setdefault(signum, handler),
                exit_process=lambda code: exit_codes.append(code),
            )
            result = []

            def target():
                result.append(daemon.run())

            t = threading.Thread(target=target, daemon=True)
            t.start()
            deadline = time.time() + 3
            while time.time() < deadline and not exit_codes:
                time.sleep(0.01)
            t.join(timeout=3)
            self.assertTrue(exit_codes and exit_codes[-1] != 0 or (result and result[0] != 0))
        finally:
            import shutil
            shutil.rmtree(tmp, ignore_errors=True)

    def test_graceful_signal_completion(self):
        tmp = tempfile.mkdtemp(prefix="test_agent_")
        try:
            config_path = _write_config(tmp, grace=300)
            clock = FakeClock()
            sleeps = []
            exit_codes = []
            signals = {}
            guardian = mock.Mock()
            notifier = mock.Mock()

            release = threading.Event()
            counter = {"n": 0}

            def run_one_issue():
                counter["n"] += 1
                release.wait(timeout=3)

            worker = mock.Mock()
            worker.running = True
            worker.current_attempts = 0
            worker.run_one_issue.side_effect = run_one_issue

            daemon = CloudAgentDaemon(
                config_path,
                gateway=mock.Mock(running=True, global_port=17699, start_background_tasks=lambda: None),
                worker=worker,
                guardian=guardian,
                notifier=notifier,
                clock=clock,
                sleep=self._sleep_factory(sleeps),
                register_signal=lambda signum, handler: signals.setdefault(signum, handler),
                exit_process=lambda code: exit_codes.append(code),
            )
            # prevent gateway thread start: owns_gateway False due to gateway injection
            result = []

            def target():
                result.append(daemon.run())

            t = threading.Thread(target=target, daemon=True)
            t.start()
            # wait for worker to start
            deadline = time.time() + 2
            while counter["n"] == 0 and time.time() < deadline:
                time.sleep(0.01)
            self.assertEqual(counter["n"], 1)
            handler = signals.get(signal.SIGTERM)
            self.assertIsNotNone(handler)
            handler(signal.SIGTERM, None)
            release.set()
            t.join(timeout=3)
            self.assertEqual(result[0] if result else None, 0)
            self.assertEqual(counter["n"], 1)
        finally:
            import shutil
            shutil.rmtree(tmp, ignore_errors=True)

    def test_deadline_expiry_through_run(self):
        tmp = tempfile.mkdtemp(prefix="test_agent_")
        try:
            config_path = _write_config(tmp, grace=0.1)
            clock = FakeClock()
            sleeps = []
            exit_codes = []
            signals = {}
            guardian = mock.Mock()
            notifier = mock.Mock()

            release = threading.Event()

            def run_one_issue():
                release.wait(timeout=5)

            worker = mock.Mock()
            worker.running = True
            worker.current_attempts = 3
            worker.run_one_issue.side_effect = run_one_issue

            daemon = CloudAgentDaemon(
                config_path,
                gateway=mock.Mock(running=True, global_port=17699, start_background_tasks=lambda: None),
                worker=worker,
                guardian=guardian,
                notifier=notifier,
                clock=clock,
                sleep=self._sleep_factory(sleeps),
                register_signal=lambda signum, handler: signals.setdefault(signum, handler),
                exit_process=lambda code: exit_codes.append(code),
            )
            result = []

            def target():
                result.append(daemon.run())

            t = threading.Thread(target=target, daemon=True)
            t.start()
            deadline = time.time() + 2
            while not (worker.run_one_issue.called if hasattr(worker.run_one_issue, "called") else True) and time.time() < deadline:
                time.sleep(0.01)
            time.sleep(0.1)
            handler = signals.get(signal.SIGTERM)
            self.assertIsNotNone(handler)
            handler(signal.SIGTERM, None)
            # advance clock past grace
            time.sleep(0.05)
            clock.advance(0.2)
            # wait for expiry handling
            time.sleep(0.3)
            self.assertEqual(worker.record_terminal_shutdown.call_count, 1)
            self.assertEqual(worker.record_terminal_shutdown.call_args, mock.call(3))
            self.assertEqual(worker.terminate_active_process_group.call_count, 1)
            # later poll/second expiry does not repeat
            daemon._handle_deadline_expiry()
            self.assertEqual(worker.record_terminal_shutdown.call_count, 1)
            # no independent session-kill hook
            self.assertFalse(guardian.kill_session.called)
            release.set()
            t.join(timeout=3)
        finally:
            import shutil
            shutil.rmtree(tmp, ignore_errors=True)

    def test_honest_shutdown_failure(self):
        tmp = tempfile.mkdtemp(prefix="test_agent_")
        try:
            config_path = _write_config(tmp, grace=0.1)
            clock = FakeClock()
            sleeps = []
            exit_codes = []
            signals = {}
            guardian = mock.Mock()
            notifier = mock.Mock()

            release = threading.Event()

            def run_one_issue():
                release.wait(timeout=5)

            worker = mock.Mock()
            worker.running = True
            worker.current_attempts = 2
            worker.run_one_issue.side_effect = run_one_issue
            worker.record_terminal_shutdown.side_effect = RuntimeError("state write failed")

            daemon = CloudAgentDaemon(
                config_path,
                gateway=mock.Mock(running=True, global_port=17699, start_background_tasks=lambda: None),
                worker=worker,
                guardian=guardian,
                notifier=notifier,
                clock=clock,
                sleep=self._sleep_factory(sleeps),
                register_signal=lambda signum, handler: signals.setdefault(signum, handler),
                exit_process=lambda code: exit_codes.append(code),
            )

            log_messages = []
            h = logging.Handler()

            class _H(logging.Handler):
                def emit(self, record):
                    log_messages.append(record.getMessage())

            handler = _H()
            logger = logging.getLogger("onespace-agent")
            logger.addHandler(handler)
            try:
                result = []

                def target():
                    result.append(daemon.run())

                t = threading.Thread(target=target, daemon=True)
                t.start()
                time.sleep(0.1)
                sig_handler = signals.get(signal.SIGTERM)
                self.assertIsNotNone(sig_handler)
                sig_handler(signal.SIGTERM, None)
                time.sleep(0.05)
                clock.advance(0.2)
                time.sleep(0.3)
                self.assertEqual(worker.terminate_active_process_group.call_count, 1)
                self.assertTrue(any("写入关闭终态失败" in m or "state write failed" in m for m in log_messages))
                release.set()
                t.join(timeout=3)
            finally:
                logger.removeHandler(handler)
        finally:
            import shutil
            shutil.rmtree(tmp, ignore_errors=True)


class TestSystemdUnit(unittest.TestCase):
    def test_systemd_unit_uses_mixed_and_infinity(self):
        service_path = Path(__file__).resolve().parents[1] / "systemd" / "onespace-cloud-agent.service"
        with open(service_path, encoding="utf-8") as f:
            text = f.read()
        self.assertIn("KillMode=mixed", text)
        self.assertIn("TimeoutStopSec=infinity", text)


import os
import subprocess
import sys
import tempfile
import shutil

# (existing imports remain)

class TestAgentTasksAggregateCLI(unittest.TestCase):
    def test_tasks_json_includes_issue_and_plan_row(self):
        tmp = tempfile.mkdtemp(prefix="agent_tasks_")
        try:
            home = os.path.join(tmp, "home")
            os.makedirs(home)
            cfg_dir = os.path.join(home, ".config", "onespace-cloud-agent")
            os.makedirs(cfg_dir, exist_ok=True)
            # issue history fixture
            issue_history = [{"id": "i1", "source": "issue", "repo": "o/r", "issue_number": 1, "title": "Issue", "status": "completed"}]
            with open(os.path.join(cfg_dir, "task_history.json"), "w") as f:
                json.dump(issue_history, f)
            config_path = os.path.join(tmp, "config.json")
            with open(config_path, "w") as f:
                json.dump({"ai_gateway": {"port": 17699}}, f)
            env = dict(os.environ)
            env["HOME"] = home
            # create a plan task in the default state root if product module exists
            try:
                import base64
                from core.plan_tasks import PlanTaskManager
                state_root = os.path.join(cfg_dir, "plan-tasks")
                os.makedirs(state_root, exist_ok=True)
                def gh_runner(cmd, *a, **k):
                    return type("P", (), {"returncode": 0, "stdout": '{"nameWithOwner": "o/r"}', "stderr": ""})()
                def tmux_runner(cmd, *a, **k):
                    return type("P", (), {"returncode": 0, "stdout": "", "stderr": ""})()
                mgr = PlanTaskManager(
                    {"plan_jobs": {"workspace_root": os.path.join(tmp, "ws")}},
                    state_root=state_root, workspace_root=os.path.join(tmp, "ws"),
                    gh_runner=gh_runner, tmux_runner=tmux_runner,
                )
                env_obj = {
                    "schema_version": 1, "request_id": "req-1",
                    "workspace": {"id": "w1", "name": "ws", "root_path": "/tmp/ws"},
                    "repository": {"remote_url": "https://example.com/o/r.git", "canonical": None, "host": "github.com", "branch": "main", "base_branch": "main", "source": "git", "head_sha": "abc", "dirty": False},
                    "plan": {"plan_id": "p1", "title": "Plan", "directory": "plan", "files": [{"path": "a.txt", "content_base64": base64.b64encode(b"hi").decode()}]},
                    "policy": {"auto_permission_once": True, "auto_recommended_questions": True, "auto_merge": False},
                }
                mgr.submit(env_obj, "req-1")
            except Exception:
                pass
            proc = subprocess.run(
                [sys.executable, "agent.py", "tasks", "--json", "--config", config_path],
                cwd=os.path.dirname(os.path.abspath(__file__)) + "/..",
                env=env, capture_output=True, text=True,
            )
            self.assertEqual(proc.returncode, 0, proc.stderr)
            data = json.loads(proc.stdout)
            tasks = data if isinstance(data, list) else data.get("tasks", [])
            self.assertTrue(any(t.get("source") != "plan" and t.get("issue_number") == 1 for t in tasks))
            plan_rows = [t for t in tasks if t.get("source") == "plan"]
            self.assertTrue(plan_rows, "expected a plan row")
            self.assertIsNone(plan_rows[0].get("issue_number"))
        finally:
            shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    unittest.main()


class TestAgentStatusReporting(unittest.TestCase):
    def test_gateway_uses_supplied_config_port(self):
        import agent
        tmp = tempfile.mkdtemp(prefix="status_")
        cfg_path = os.path.join(tmp, "config.json")
        with open(cfg_path, "w", encoding="utf-8") as f:
            json.dump({"ai_gateway": {"port": 18000}}, f)
        with mock.patch("agent.subprocess.run") as run_mock:
            run_mock.return_value = mock.Mock(stdout="active\n", stderr="", returncode=0)
            with mock.patch("agent.urllib.request.urlopen") as open_mock:
                resp = mock.Mock()
                resp.status = 200
                resp.read.return_value = b'{"status":"ok"}'
                open_mock.return_value.__enter__.return_value = resp
                report = agent.get_status_report(cfg_path)
        self.assertTrue(open_mock.called)
        open_mock.assert_called()
        args = open_mock.call_args[0][0] if open_mock.call_args[0] else open_mock.call_args.args[0]
        self.assertIn("18000", getattr(args, "full_url", str(args)))

    def test_inactive_systemd_yields_false(self):
        import agent
        tmp = tempfile.mkdtemp(prefix="status_")
        cfg_path = os.path.join(tmp, "config.json")
        with open(cfg_path, "w", encoding="utf-8") as f:
            json.dump({"ai_gateway": {"port": 17688}}, f)
        with mock.patch("agent.subprocess.run") as run_mock:
            run_mock.return_value = mock.Mock(stdout="inactive\n", stderr="", returncode=3)
            with mock.patch("agent.urllib.request.urlopen", side_effect=Exception("down")):
                report = agent.get_status_report(cfg_path)
        self.assertFalse(report["service_active"])

    def test_systemctl_exception_yields_false(self):
        import agent
        tmp = tempfile.mkdtemp(prefix="status_")
        cfg_path = os.path.join(tmp, "config.json")
        with open(cfg_path, "w", encoding="utf-8") as f:
            json.dump({"ai_gateway": {"port": 17688}}, f)
        with mock.patch("agent.subprocess.run", side_effect=OSError("no systemctl")):
            with mock.patch("agent.urllib.request.urlopen", side_effect=Exception("down")):
                report = agent.get_status_report(cfg_path)
        self.assertFalse(report["service_active"])
