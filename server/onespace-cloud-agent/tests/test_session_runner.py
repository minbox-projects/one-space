#!/usr/bin/env python3
"""
session_runner.py 单元测试
"""

import os
import shutil
import tempfile
import unittest
from unittest.mock import patch
from core.session_runner import SessionRunner


class TestSessionRunner(unittest.TestCase):
    def setUp(self):
        self.tmp_ws = tempfile.mkdtemp(prefix="test_ws_")
        self.runner = SessionRunner(workspace_root=self.tmp_ws)

    def tearDown(self):
        if os.path.exists(self.tmp_ws):
            shutil.rmtree(self.tmp_ws, ignore_errors=True)

    def test_sanitize_name(self):
        self.assertEqual(self.runner._sanitize_name("my-repo.git"), "my-repo_git")
        self.assertEqual(self.runner._sanitize_name("user/project_name"), "user_project_name")
        self.assertEqual(self.runner._sanitize_name("normal_name-123"), "normal_name-123")

    @patch("os.execvp")
    def test_auto_create_nonexistent_workspace(self, mock_execvp):
        target_name = "brand-new-project"
        self.runner.start_or_attach(target_name)
        expected_dir = os.path.join(self.tmp_ws, target_name)
        self.assertTrue(os.path.exists(expected_dir))
        mock_execvp.assert_called_once()
        cmd_args = mock_execvp.call_args[0][1]
        self.assertIn("dev_brand-new-project", cmd_args)


if __name__ == "__main__":
    unittest.main()
