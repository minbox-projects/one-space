#!/usr/bin/env python3
"""
issue_worker.py 单元测试
"""

import os
import tempfile
import unittest
from core.disk_guardian import DiskGuardian
from core.issue_worker import IssueWorker
from core.notifier import Notifier


class TestIssueWorker(unittest.TestCase):
    def setUp(self):
        self.tmp_dir = tempfile.mkdtemp(prefix="test_worker_")
        self.cfg = {
            "issue_worker": {
                "enabled": True,
                "watch_repositories": ["test/repo"],
                "trigger_label": "bot:task",
                "poll_interval_seconds": 60,
                "workspace_root": self.tmp_dir
            }
        }
        self.guardian = DiskGuardian(opencode_dir=self.tmp_dir)
        self.notifier = Notifier(self.cfg)
        self.worker = IssueWorker(self.cfg, self.guardian, self.notifier)
        # 覆盖 state 文件路径到临时目录
        self.worker.state_file = os.path.join(self.tmp_dir, "worker_state.json")

    def tearDown(self):
        import shutil
        if os.path.exists(self.tmp_dir):
            shutil.rmtree(self.tmp_dir, ignore_errors=True)

    def test_processed_state_persistence(self):
        self.assertFalse(self.worker.is_issue_processed("test/repo", 101))
        self.worker.mark_issue_processed("test/repo", 101)
        self.assertTrue(self.worker.is_issue_processed("test/repo", 101))

        # 重新创建实例，验证持久化
        worker2 = IssueWorker(self.cfg, self.guardian, self.notifier)
        worker2.state_file = self.worker.state_file
        worker2.load_state()
        self.assertTrue(worker2.is_issue_processed("test/repo", 101))


if __name__ == "__main__":
    unittest.main()
