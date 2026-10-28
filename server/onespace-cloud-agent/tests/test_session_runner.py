#!/usr/bin/env python3
"""
session_runner.py 单元测试
"""

import unittest
from core.session_runner import SessionRunner


class TestSessionRunner(unittest.TestCase):
    def setUp(self):
        self.runner = SessionRunner(workspace_root="/tmp/test-cloud-agent-workspaces")

    def test_sanitize_name(self):
        self.assertEqual(self.runner._sanitize_name("my-repo.git"), "my-repo_git")
        self.assertEqual(self.runner._sanitize_name("user/project_name"), "user_project_name")
        self.assertEqual(self.runner._sanitize_name("normal_name-123"), "normal_name-123")


if __name__ == "__main__":
    unittest.main()
