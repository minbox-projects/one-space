#!/usr/bin/env python3
"""
session_runner.py 单元测试
"""

import os
import shutil
import tempfile
import unittest
from unittest.mock import patch, MagicMock

from core.session_runner import SessionRunner


def _side_effect(branch, has_origin=True, fetch_ok=True, branch_exists=False,
                 symref_out=None, origin_main_exists=True, origin_master_exists=False):
    def _run(cmd, *args, **kwargs):
        cmd = list(cmd)
        if "remote" in cmd:
            m = MagicMock()
            if not has_origin:
                m.returncode = 1
                m.stdout = ""
            else:
                m.returncode = 0
                m.stdout = "https://example.com/repo.git\n" if "get-url" in cmd else "origin\n"
            m.stderr = ""
            return m
        if "fetch" in cmd:
            m = MagicMock()
            m.returncode = 0 if fetch_ok else 1
            m.stdout = ""
            m.stderr = "" if fetch_ok else "fatal: fetch failed"
            return m
        if "symbolic-ref" in cmd:
            m = MagicMock()
            if symref_out:
                m.returncode = 0
                m.stdout = symref_out + "\n"
            else:
                m.returncode = 1
                m.stdout = ""
            m.stderr = ""
            return m
        if "rev-parse" in cmd:
            m = MagicMock()
            target = None
            if "--verify" in cmd:
                idx = cmd.index("--verify")
                if idx + 1 < len(cmd):
                    target = cmd[idx + 1]
            if target == branch:
                m.returncode = 0 if branch_exists else 1
            elif target == "origin/main":
                m.returncode = 0 if origin_main_exists else 1
            elif target == "origin/master":
                m.returncode = 0 if origin_master_exists else 1
            else:
                m.returncode = 0
            m.stdout = ""
            m.stderr = ""
            return m
        m = MagicMock()
        m.returncode = 0
        m.stdout = ""
        m.stderr = ""
        return m
    return _run


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
    @patch("core.session_runner.subprocess.run")
    def test_feature_branch_worktree_isolation(self, mock_subproc, mock_execvp):
        repo_dir = os.path.join(self.tmp_ws, "test-repo")
        os.makedirs(repo_dir, exist_ok=True)
        mock_subproc.side_effect = _side_effect("feat-auth", has_origin=True, branch_exists=False)
        self.runner.start_or_attach("test-repo", "feat-auth")
        mock_execvp.assert_called_once()
        cmd_args = mock_execvp.call_args[0][1]
        self.assertIn("dev_test-repo_feat-auth", cmd_args)
        expected_wt_dir = os.path.join(self.tmp_ws, "test-repo-worktrees", "feat-auth")
        self.assertIn(expected_wt_dir, cmd_args)

    @patch("core.session_runner.subprocess.run")
    def test_prepare_worktree_reuses_existing_dir_no_fetch(self, mock_subproc):
        repo_dir = os.path.join(self.tmp_ws, "test-repo")
        os.makedirs(repo_dir, exist_ok=True)
        worktree_dir = os.path.join(self.tmp_ws, "test-repo-worktrees", "feat-auth")
        os.makedirs(worktree_dir, exist_ok=True)
        result = self.runner._prepare_worktree(repo_dir, "feat-auth")
        self.assertEqual(result, worktree_dir)
        mock_subproc.assert_not_called()

    @patch("core.session_runner.subprocess.run")
    def test_prepare_worktree_fetch_failure_raises(self, mock_subproc):
        repo_dir = os.path.join(self.tmp_ws, "test-repo")
        os.makedirs(repo_dir, exist_ok=True)
        mock_subproc.side_effect = _side_effect("feat-auth", has_origin=True, fetch_ok=False)
        with self.assertRaises(RuntimeError):
            self.runner._prepare_worktree(repo_dir, "feat-auth")
        calls = [list(c.args[0]) for c in mock_subproc.call_args_list]
        self.assertFalse(any("worktree" in c and "add" in c for c in calls))

    @patch("core.session_runner.subprocess.run")
    def test_prepare_worktree_no_origin_skips_fetch(self, mock_subproc):
        repo_dir = os.path.join(self.tmp_ws, "test-repo")
        os.makedirs(repo_dir, exist_ok=True)
        mock_subproc.side_effect = _side_effect("feat-auth", has_origin=False, branch_exists=False, symref_out=None)
        self.runner._prepare_worktree(repo_dir, "feat-auth")
        calls = [list(c.args[0]) for c in mock_subproc.call_args_list]
        self.assertFalse(any("fetch" in c for c in calls))

    @patch("core.session_runner.subprocess.run")
    def test_prepare_worktree_local_branch_exists_no_startpoint(self, mock_subproc):
        repo_dir = os.path.join(self.tmp_ws, "test-repo")
        os.makedirs(repo_dir, exist_ok=True)
        mock_subproc.side_effect = _side_effect("feat-auth", has_origin=True, branch_exists=True)
        self.runner._prepare_worktree(repo_dir, "feat-auth")
        calls = [list(c.args[0]) for c in mock_subproc.call_args_list]
        wt_cmds = [c for c in calls if "worktree" in c and "add" in c]
        self.assertTrue(wt_cmds)
        cmd = wt_cmds[0]
        self.assertIn("feat-auth", cmd)
        self.assertNotIn("-b", cmd)

    @patch("core.session_runner.subprocess.run")
    def test_prepare_worktree_new_branch_based_on_origin_head(self, mock_subproc):
        repo_dir = os.path.join(self.tmp_ws, "test-repo")
        os.makedirs(repo_dir, exist_ok=True)
        mock_subproc.side_effect = _side_effect("feat-auth", has_origin=True, branch_exists=False, symref_out="origin/main")
        self.runner._prepare_worktree(repo_dir, "feat-auth")
        calls = [list(c.args[0]) for c in mock_subproc.call_args_list]
        fetch_idx = next(i for i, c in enumerate(calls) if "fetch" in c)
        wt_idx = next(i for i, c in enumerate(calls) if "worktree" in c and "add" in c)
        self.assertLess(fetch_idx, wt_idx)
        wt_cmds = [c for c in calls if "worktree" in c and "add" in c]
        cmd = wt_cmds[0]
        self.assertIn("-b", cmd)
        self.assertIn("feat-auth", cmd)
        self.assertIn("origin/main", cmd)

    @patch("core.session_runner.subprocess.run")
    def test_prepare_worktree_origin_head_missing_fallback_origin_main(self, mock_subproc):
        repo_dir = os.path.join(self.tmp_ws, "test-repo")
        os.makedirs(repo_dir, exist_ok=True)
        mock_subproc.side_effect = _side_effect("feat-auth", has_origin=True, branch_exists=False,
                                                symref_out=None, origin_main_exists=True)
        self.runner._prepare_worktree(repo_dir, "feat-auth")
        calls = [list(c.args[0]) for c in mock_subproc.call_args_list]
        wt_cmds = [c for c in calls if "worktree" in c and "add" in c]
        cmd = wt_cmds[0]
        self.assertIn("-b", cmd)
        self.assertIn("origin/main", cmd)


if __name__ == "__main__":
    unittest.main()
