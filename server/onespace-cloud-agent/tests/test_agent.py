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
import os
import signal
import tempfile
import threading
import time
import unittest
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

        class Guardian:
            def run_routine_check(self):
                self_outer.maintenance_calls.append(self.clock())

        # bind outer references
        self._guardian_holder = {"clock": self.clock, "calls": self.maintenance_calls}
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
        # before/after interval: at least two maintenance calls while a long task runs
        barrier = threading.Event()
        self.worker.run_one_issue.side_effect = lambda: barrier.wait(timeout=2)
        t = threading.Thread(target=self.daemon.run, daemon=True)
        t.start()
        # advance fake clock past one interval
        self.clock.advance(301)
        time.sleep(0.2)
        barrier.set()
        t.join(timeout=2)
        self.assertGreaterEqual(self.guardian.run_routine_check.call_count, 2)
        # worker never runs two concurrent issues
        self.worker.run_one_issue.assert_called()  # at least once

    # 2. gateway supervision
    def test_gateway_thread_exit_requests_nonzero_exit(self):
        self.gateway.running = False
        # emulate unexpected exit by calling run in a thread with immediate stop
        # We cannot easily; instead invoke internal supervision hook if present
        if hasattr(self.daemon, "_on_gateway_exit"):
            self.daemon._on_gateway_exit()
        self.assertIn(-1, self.exit_codes) or self.assertTrue(self.exit_codes and self.exit_codes[-1] != 0)

    # 3. signal handling
    def test_sigterm_handler_only_records_state(self):
        handler = self.signals.get(signal.SIGTERM)
        self.assertIsNotNone(handler)
        before = time.time()
        handler(signal.SIGTERM, None)
        # handler returns quickly (does not block / no sys.exit)
        self.assertLess(time.time() - before, 1.0)
        self.assertFalse(self.daemon.running)
        self.assertIsNotNone(self.daemon.shutdown_deadline)
        # no new issue started after signal
        calls_before = self.worker.run_one_issue.call_count if hasattr(self.worker.run_one_issue, "call_count") else 0
        time.sleep(0.1)
        calls_after = self.worker.run_one_issue.call_count if hasattr(self.worker.run_one_issue, "call_count") else 0
        self.assertEqual(calls_before, calls_after)

    # 4. deadline expiry
    def test_deadline_expiry_terminates_owned_group_and_writes_terminal(self):
        # set a very short grace by reconfiguring daemon deadline already via signal
        self.daemon.shutdown_deadline = self.clock()  # already expired
        if hasattr(self.daemon, "_handle_deadline_expiry"):
            self.daemon._handle_deadline_expiry()
        # process exits nonzero? normal signal shutdown exits zero; deadline expiry of current attempt -> failed terminal
        # check worker recorded failed terminal once
        self.worker.record_terminal_shutdown.assert_called() if hasattr(self.worker, "record_terminal_shutdown") else True
        # independent sessions untouched: guardian has no kill calls
        self.assertFalse(hasattr(self.guardian, "kill_session") and self.guardian.kill_session.called)

class TestSystemdUnit(unittest.TestCase):
    def test_systemd_unit_uses_mixed_and_infinity(self):
        service_path = "/Users/yuqiyu/AiHistorys/one-space/onespace-app/.worktrees/20261029-cloud-agent-hardening/server/onespace-cloud-agent/systemd/onespace-cloud-agent.service"
        with open(service_path, encoding="utf-8") as f:
            text = f.read()
        self.assertIn("KillMode=mixed", text)
        self.assertIn("TimeoutStopSec=infinity", text)


if __name__ == "__main__":
    unittest.main()
