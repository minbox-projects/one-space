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

    @patch("os.execvp")
    @patch("subprocess.run")
    def test_feature_branch_worktree_isolation(self, mock_subproc, mock_execvp):
        # 模拟主仓库存在
        repo_dir = os.path.join(self.tmp_ws, "test-repo")
        os.makedirs(repo_dir, exist_ok=True)

        # 模拟 git 命令返回
        mock_res = unittest.mock.MagicMock()
        mock_res.returncode = 0
        mock_subproc.return_value = mock_res

        self.runner.start_or_attach("test-repo", "feat-auth")

        mock_execvp.assert_called_once()
        cmd_args = mock_execvp.call_args[0][1]
        self.assertIn("dev_test-repo_feat-auth", cmd_args)
        expected_wt_dir = os.path.join(self.tmp_ws, "test-repo-worktrees", "feat-auth")
        self.assertIn(expected_wt_dir, cmd_args)


if __name__ == "__main__":
    unittest.main()
