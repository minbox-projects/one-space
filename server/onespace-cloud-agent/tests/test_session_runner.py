#!/usr/bin/env python3
"""
session_runner.py 单元测试
"""

import os
import shutil
import shlex
import shutil
import tempfile
import unittest
from unittest.mock import patch, MagicMock

from core.session_runner import SessionRunner


def _side_effect(branch, has_origin=True, fetch_ok=True, branch_exists=False,
                 symref_out=None, origin_main_exists=True, origin_master_exists=False,
                 check_ref_ok=True, registered_paths=None):
    registered_paths = registered_paths or []

    def _run(cmd, *args, **kwargs):
        cmd = list(cmd)
        if "check-ref-format" in cmd:
            m = MagicMock()
            m.returncode = 0 if check_ref_ok else 1
            m.stdout = ""
            m.stderr = "" if check_ref_ok else "fatal: invalid branch"
            return m
        if "worktree" in cmd and "list" in cmd:
            m = MagicMock()
            m.returncode = 0
            lines = []
            for p in registered_paths:
                lines.append(f"worktree {p}")
                lines.append("HEAD 0000000000000000000000000000000000000000")
                lines.append(f"branch refs/heads/{branch}")
                lines.append("")
            m.stdout = "\n".join(lines)
            m.stderr = ""
            return m
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
        mock_subproc.side_effect = _side_effect(
            "feat-auth", registered_paths=[worktree_dir]
        )
        result = self.runner._prepare_worktree(repo_dir, "feat-auth")
        self.assertEqual(result, worktree_dir)
        calls = [list(c.args[0]) for c in mock_subproc.call_args_list]
        self.assertFalse(any("worktree" in c and "add" in c for c in calls))

    @patch("core.session_runner.subprocess.run")
    def test_prepare_worktree_unregistered_existing_dir_fails(self, mock_subproc):
        repo_dir = os.path.join(self.tmp_ws, "test-repo")
        os.makedirs(repo_dir, exist_ok=True)
        worktree_dir = os.path.join(self.tmp_ws, "test-repo-worktrees", "feat-auth")
        os.makedirs(worktree_dir, exist_ok=True)
        mock_subproc.side_effect = _side_effect("feat-auth", registered_paths=[])
        with self.assertRaises(RuntimeError):
            self.runner._prepare_worktree(repo_dir, "feat-auth")

    @patch("core.session_runner.subprocess.run")
    def test_prepare_worktree_partial_dir_after_failed_add_fails(self, mock_subproc):
        repo_dir = os.path.join(self.tmp_ws, "test-repo")
        os.makedirs(repo_dir, exist_ok=True)
        worktree_dir = os.path.join(self.tmp_ws, "test-repo-worktrees", "feat-auth")
        os.makedirs(worktree_dir, exist_ok=True)
        mock_subproc.side_effect = _side_effect("feat-auth", registered_paths=[])
        with self.assertRaises(RuntimeError):
            self.runner._prepare_worktree(repo_dir, "feat-auth")

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

    @patch("core.session_runner.subprocess.run")
    def test_branch_leading_dash_fails_before_git(self, mock_subproc):
        repo_dir = os.path.join(self.tmp_ws, "test-repo")
        os.makedirs(repo_dir, exist_ok=True)
        with self.assertRaises(RuntimeError):
            self.runner._prepare_worktree(repo_dir, "-bad")
        mock_subproc.assert_not_called()

    @patch("core.session_runner.subprocess.run")
    def test_branch_space_reaches_check_ref_format_and_fails(self, mock_subproc):
        repo_dir = os.path.join(self.tmp_ws, "test-repo")
        os.makedirs(repo_dir, exist_ok=True)
        mock_subproc.side_effect = _side_effect("feat x", check_ref_ok=False)
        with self.assertRaises(RuntimeError):
            self.runner._prepare_worktree(repo_dir, "feat x")
        calls = [list(c.args[0]) for c in mock_subproc.call_args_list]
        self.assertTrue(any("check-ref-format" in c and "--branch" in c and "feat x" in c for c in calls))

    @patch("core.session_runner.subprocess.run")
    def test_branch_literal_accepted(self, mock_subproc):
        repo_dir = os.path.join(self.tmp_ws, "test-repo")
        os.makedirs(repo_dir, exist_ok=True)
        for branch in ("feat/x", "feat;rm"):
            mock_subproc.side_effect = _side_effect(branch, has_origin=True, branch_exists=False, symref_out="origin/main")
            result = self.runner._prepare_worktree(repo_dir, branch)
            sanitized = self.runner._sanitize_name(branch)
            expected_dir = os.path.join(self.tmp_ws, "test-repo-worktrees", sanitized)
            self.assertEqual(result, expected_dir)
            calls = [list(c.args[0]) for c in mock_subproc.call_args_list]
            self.assertTrue(any(branch in c for c in calls))

    @patch("core.session_runner.subprocess.run")
    def test_list_sessions_detailed(self, mock_subproc):
        mock_res = MagicMock()
        mock_res.returncode = 0
        mock_res.stdout = "dev_onespace_feat-login|1700000000|2|/root/workspaces/login|9876\n"
        mock_subproc.return_value = mock_res

        sessions = self.runner.list_sessions_detailed()
        self.assertEqual(len(sessions), 1)
        s = sessions[0]
        self.assertEqual(s["session_name"], "dev_onespace_feat-login")
        self.assertEqual(s["repo_name"], "onespace")
        self.assertEqual(s["branch"], "feat-login")
        self.assertEqual(s["created_at"], 1700000000)
        self.assertEqual(s["windows_count"], 2)
        self.assertEqual(s["working_dir"], "/root/workspaces/login")
        self.assertEqual(s["pane_pid"], 9876)
        self.assertTrue(s["is_active"])

    def test_list_worktrees_detailed(self):
        wt_base = os.path.join(self.tmp_ws, "my-repo-worktrees")
        branch_dir = os.path.join(wt_base, "feat-test")
        os.makedirs(branch_dir, exist_ok=True)

        wts = self.runner.list_worktrees_detailed()
        self.assertEqual(len(wts), 1)
        self.assertEqual(wts[0]["repo_name"], "my-repo")
        self.assertEqual(wts[0]["branch"], "feat-test")
        self.assertEqual(wts[0]["worktree_path"], branch_dir)
        self.assertFalse(wts[0]["has_active_session"])

    @patch("core.session_runner.subprocess.run")
    def test_start_or_attach_no_attach_headless(self, mock_subproc):
        repo_dir = os.path.join(self.tmp_ws, "test-repo")
        os.makedirs(repo_dir, exist_ok=True)
        mock_res = MagicMock()
        mock_res.returncode = 0
        mock_subproc.return_value = mock_res

        res = self.runner.start_or_attach("test-repo", use_yolo=False, extra_args="--model test-model", attach=False)
        self.assertTrue(res["success"])
        self.assertEqual(res["session_name"], "dev_test-repo")

        calls = [list(c.args[0]) for c in mock_subproc.call_args_list]
        tmux_calls = [c for c in calls if "tmux" in c and "new-session" in c]
        self.assertTrue(tmux_calls)
        cmd = tmux_calls[0]
        self.assertIn("-d", cmd)
        full_init_cmd = cmd[-1]
        self.assertNotIn("--yolo", full_init_cmd)
        self.assertIn("--model test-model", full_init_cmd)

    @patch("os.execvp")
    @patch.object(SessionRunner, "session_exists", return_value=False)
    @patch("core.session_runner.subprocess.run")
    def test_restore_named_session_reuses_registered_worktree(self, mock_run, _mock_exists, mock_execvp):
        wt_dir = os.path.join(self.tmp_ws, "one-space-worktrees", "test1")
        os.makedirs(wt_dir, exist_ok=True)
        mock_run.side_effect = _side_effect("test1", registered_paths=[wt_dir])
        res = self.runner.start_or_attach("dev_one-space_test1")
        self.assertTrue(res["success"])
        self.assertEqual(res["session_name"], "dev_one-space_test1")
        mock_execvp.assert_called_once()
        cmd = mock_execvp.call_args[0][1]
        self.assertEqual(cmd[0:3], ["tmux", "new-session", "-s"])
        self.assertEqual(cmd[3], "dev_one-space_test1")
        self.assertEqual(cmd[5], wt_dir)

    @patch("os.execvp")
    @patch.object(SessionRunner, "session_exists", return_value=False)
    @patch("core.session_runner.subprocess.run")
    def test_restore_named_session_unregistered_fails(self, mock_run, _mock_exists, mock_execvp):
        wt_dir = os.path.join(self.tmp_ws, "one-space-worktrees", "test1")
        os.makedirs(wt_dir, exist_ok=True)
        mock_run.side_effect = _side_effect("test1", registered_paths=[])
        with self.assertRaises(RuntimeError):
            self.runner.start_or_attach("dev_one-space_test1")
        mock_execvp.assert_not_called()

    @patch("os.execvp")
    @patch.object(SessionRunner, "session_exists", return_value=False)
    def test_restore_named_session_without_workspace_raises(self, _mock_exists, _mock_execvp):
        with self.assertRaises(RuntimeError):
            self.runner.start_or_attach("dev_missing")
        self.assertFalse(os.path.exists(os.path.join(self.tmp_ws, "dev_missing")))

    # Literal argv / init command tests
    @patch("shutil.which", return_value="/usr/bin/opencode")
    def test_build_init_command_default_exact(self, _which):
        init_cmd = self.runner._build_init_command("/tmp/project dir", True, "")
        self.assertEqual(init_cmd, shlex.join(["/usr/bin/opencode", "--yolo"]))

    @patch("shutil.which", return_value=None)
    @patch("os.path.exists", return_value=True)
    def test_build_init_command_fallback_executable(self, _exists, _which):
        init_cmd = self.runner._build_init_command("/tmp/project dir", True, "")
        self.assertEqual(init_cmd, shlex.join(["/root/.opencode/bin/opencode", "--yolo"]))

    @patch("shutil.which", return_value="/usr/bin/opencode")
    def test_build_init_command_no_yolo_and_extra_args(self, _which):
        init_cmd = self.runner._build_init_command("/tmp/project dir", False, "--model test-model extra")
        self.assertEqual(init_cmd, shlex.join(["/usr/bin/opencode", *shlex.split("--model test-model extra")]))
        self.assertNotIn("--yolo", init_cmd)

    @patch("shutil.which", return_value="/usr/bin/opencode")
    def test_build_init_command_no_fallback_shell(self, _which):
        init_cmd = self.runner._build_init_command("/tmp/project dir", True, "")
        self.assertNotIn("exec bash -l", init_cmd)
        self.assertNotIn("cd ", init_cmd)

    @patch("os.execvp")
    @patch.object(SessionRunner, "session_exists", return_value=False)
    @patch("shutil.which", return_value="/usr/bin/opencode")
    def test_spawn_session_project_path_only_in_tmux_c(self, _which, _exists, mock_execvp):
        self.runner.start_or_attach("brand-new-project")
        cmd = mock_execvp.call_args[0][1]
        self.assertEqual(cmd[0:2], ["tmux", "new-session"])
        # find -c
        idx = cmd.index("-c")
        self.assertEqual(cmd[idx + 1], os.path.join(self.tmp_ws, "brand-new-project"))
        init_cmd = cmd[-1]
        self.assertEqual(init_cmd, shlex.join(["/usr/bin/opencode", "--yolo"]))
        self.assertNotIn(os.path.join(self.tmp_ws, "brand-new-project"), init_cmd)

    @patch("os.execvp")
    @patch.object(SessionRunner, "session_exists", return_value=False)
    def test_session_survives_opencode_exit_without_fallback_shell(self, _mock_exists, mock_execvp):
        self.runner.start_or_attach("brand-new-project")
        init_cmd = mock_execvp.call_args[0][1][-1]
        self.assertIn("opencode", init_cmd)
        self.assertNotIn("exec bash -l", init_cmd)


if __name__ == "__main__":
    unittest.main()


class TestWorktreeKillCleanup(unittest.TestCase):
    def setUp(self):
        self.tmp_ws = tempfile.mkdtemp(prefix="test_wt_kill_")
        self.runner = SessionRunner(workspace_root=self.tmp_ws)

    def tearDown(self):
        if os.path.exists(self.tmp_ws):
            shutil.rmtree(self.tmp_ws, ignore_errors=True)

    def _make_pane_path(self, sub=None):
        root = os.path.join(self.tmp_ws, "repo-worktrees", "branch")
        target = root if sub is None else os.path.join(root, sub)
        os.makedirs(target, exist_ok=True)
        return target, root

    def _tmux_list_output(self, pane_path):
        return f"sess1|1|1|{pane_path}|1\n"

    @patch("core.session_runner.subprocess.run")
    def test_kill_cleans_eligible_worktree(self, mock_run):
        pane_path, root = self._make_pane_path("src")
        other_panes = []

        def side_effect(cmd, *a, **k):
            cmd = list(cmd)
            m = MagicMock()
            if cmd[:2] == ["tmux", "list-sessions"]:
                m.returncode = 0
                m.stdout = self._tmux_list_output(pane_path)
                m.stderr = ""
                return m
            if cmd[:3] == ["tmux", "kill-session"]:
                m.returncode = 0
                m.stdout = ""
                m.stderr = ""
                return m
            if "worktree" in cmd and "list" in cmd:
                m.returncode = 0
                m.stdout = f"worktree {root}\nHEAD 0\nbranch refs/heads/branch\n\n"
                m.stderr = ""
                return m
            if cmd[:4] == ["git", "-C", root, "status"]:
                m.returncode = 0
                m.stdout = ""
                m.stderr = ""
                return m
            if "worktree" in cmd and "remove" in cmd:
                m.returncode = 0
                m.stdout = ""
                m.stderr = ""
                return m
            m.returncode = 0
            m.stdout = ""
            m.stderr = ""
            return m

        mock_run.side_effect = side_effect
        ok = self.runner.kill_session("sess1")
        self.assertTrue(ok)
        cmds = [list(c.args[0]) for c in mock_run.call_args_list]
        # argv lists, no shell strings
        for c in cmds:
            self.assertIsInstance(c, list)
        remove_cmds = [c for c in cmds if "worktree" in c and "remove" in c]
        self.assertTrue(remove_cmds)
        self.assertIn(root, remove_cmds[0])
        self.assertNotIn("--force", remove_cmds[0])
        # branch not deleted
        self.assertFalse(any("branch" in c and "-D" in c for c in cmds))

    @patch("core.session_runner.subprocess.run")
    def test_kill_does_not_clean_when_other_pane_uses(self, mock_run):
        pane_path, root = self._make_pane_path()
        # two panes same root
        def side_effect(cmd, *a, **k):
            cmd = list(cmd)
            m = MagicMock()
            if cmd[:2] == ["tmux", "list-sessions"]:
                m.returncode = 0
                m.stdout = f"sess1|1|1|{root}|1\nsess2|1|1|{root}/sub|1\n"
                m.stderr = ""
                return m
            if cmd[:3] == ["tmux", "kill-session"]:
                m.returncode = 0; m.stdout=""; m.stderr=""; return m
            if "worktree" in cmd and "list" in cmd:
                m.returncode = 0
                m.stdout = f"worktree {root}\nHEAD 0\nbranch refs/heads/branch\n\n"
                m.stderr = ""; return m
            if cmd[:4] == ["git", "-C", root, "status"]:
                m.returncode = 0; m.stdout=""; m.stderr=""; return m
            m.returncode = 0; m.stdout=""; m.stderr=""; return m
        mock_run.side_effect = side_effect
        self.runner.kill_session("sess1")
        cmds = [list(c.args[0]) for c in mock_run.call_args_list]
        self.assertFalse(any("worktree" in c and "remove" in c for c in cmds))

    @patch("core.session_runner.subprocess.run")
    def test_kill_does_not_clean_dirty(self, mock_run):
        pane_path, root = self._make_pane_path()
        def side_effect(cmd, *a, **k):
            cmd = list(cmd)
            m = MagicMock()
            if cmd[:2] == ["tmux", "list-sessions"]:
                m.returncode = 0; m.stdout = f"sess1|1|1|{pane_path}|1\n"; m.stderr=""; return m
            if cmd[:3] == ["tmux", "kill-session"]:
                m.returncode = 0; m.stdout=""; m.stderr=""; return m
            if "worktree" in cmd and "list" in cmd:
                m.returncode = 0; m.stdout = f"worktree {root}\nHEAD 0\nbranch refs/heads/branch\n\n"; m.stderr=""; return m
            if cmd[:4] == ["git", "-C", root, "status"]:
                m.returncode = 0; m.stdout = " M file\n"; m.stderr=""; return m
            m.returncode = 0; m.stdout=""; m.stderr=""; return m
        mock_run.side_effect = side_effect
        self.runner.kill_session("sess1")
        cmds = [list(c.args[0]) for c in mock_run.call_args_list]
        self.assertFalse(any("worktree" in c and "remove" in c for c in cmds))

    @patch("core.session_runner.subprocess.run")
    def test_kill_does_not_clean_out_of_root(self, mock_run):
        pane_path = "/tmp/elsewhere/wt"
        root = "/tmp/elsewhere/wt"
        os.makedirs(pane_path, exist_ok=True)
        def side_effect(cmd, *a, **k):
            cmd = list(cmd)
            m = MagicMock()
            if cmd[:2] == ["tmux", "list-sessions"]:
                m.returncode = 0; m.stdout = f"sess1|1|1|{pane_path}|1\n"; m.stderr=""; return m
            if cmd[:3] == ["tmux", "kill-session"]:
                m.returncode = 0; m.stdout=""; m.stderr=""; return m
            m.returncode = 0; m.stdout=""; m.stderr=""; return m
        mock_run.side_effect = side_effect
        self.runner.kill_session("sess1")
        cmds = [list(c.args[0]) for c in mock_run.call_args_list]
        self.assertFalse(any("worktree" in c and "remove" in c for c in cmds))

    @patch("core.session_runner.subprocess.run")
    def test_kill_does_not_clean_unregistered(self, mock_run):
        pane_path, root = self._make_pane_path()
        def side_effect(cmd, *a, **k):
            cmd = list(cmd)
            m = MagicMock()
            if cmd[:2] == ["tmux", "list-sessions"]:
                m.returncode = 0; m.stdout = f"sess1|1|1|{pane_path}|1\n"; m.stderr=""; return m
            if cmd[:3] == ["tmux", "kill-session"]:
                m.returncode = 0; m.stdout=""; m.stderr=""; return m
            if "worktree" in cmd and "list" in cmd:
                m.returncode = 0; m.stdout = ""; m.stderr=""; return m
            m.returncode = 0; m.stdout=""; m.stderr=""; return m
        mock_run.side_effect = side_effect
        self.runner.kill_session("sess1")
        cmds = [list(c.args[0]) for c in mock_run.call_args_list]
        self.assertFalse(any("worktree" in c and "remove" in c for c in cmds))

    @patch("core.session_runner.subprocess.run")
    def test_kill_does_not_clean_when_inspection_fails(self, mock_run):
        pane_path, root = self._make_pane_path()
        def side_effect(cmd, *a, **k):
            cmd = list(cmd)
            m = MagicMock()
            if cmd[:2] == ["tmux", "list-sessions"]:
                m.returncode = 0; m.stdout = f"sess1|1|1|{pane_path}|1\n"; m.stderr=""; return m
            if cmd[:3] == ["tmux", "kill-session"]:
                m.returncode = 0; m.stdout=""; m.stderr=""; return m
            if "worktree" in cmd and "list" in cmd:
                m.returncode = 1; m.stdout=""; m.stderr="err"; return m
            m.returncode = 0; m.stdout=""; m.stderr=""; return m
        mock_run.side_effect = side_effect
        self.runner.kill_session("sess1")
        cmds = [list(c.args[0]) for c in mock_run.call_args_list]
        self.assertFalse(any("worktree" in c and "remove" in c for c in cmds))
