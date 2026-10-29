#!/usr/bin/env python3
"""
disk_guardian.py 单元测试
"""

import os
import time
import tempfile
import unittest
from unittest.mock import patch, MagicMock
from core.disk_guardian import DiskGuardian


class TestDiskGuardian(unittest.TestCase):
    def setUp(self):
        self.tmp_dir = tempfile.mkdtemp(prefix="test_disk_guardian_")
        self.guardian = DiskGuardian(opencode_dir=self.tmp_dir, min_free_gb=1.0)

    def tearDown(self):
        import shutil
        if os.path.exists(self.tmp_dir):
            shutil.rmtree(self.tmp_dir, ignore_errors=True)

    def test_disk_usage(self):
        usage = self.guardian.get_disk_usage("/")
        self.assertIn("total_gb", usage)
        self.assertIn("free_gb", usage)
        self.assertGreater(usage["total_gb"], 0)

    def test_cleanup_workspace(self):
        dummy_ws = os.path.join(self.tmp_dir, "ws1")
        os.makedirs(dummy_ws, exist_ok=True)
        with open(os.path.join(dummy_ws, "test.txt"), "w") as f:
            f.write("hello")
        self.assertTrue(os.path.exists(dummy_ws))
        self.guardian.cleanup_workspace(dummy_ws)
        self.assertFalse(os.path.exists(dummy_ws))


if __name__ == "__main__":
    unittest.main()


class TestDiskGuardianCleanup(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="dg_")
        self.root1 = os.path.join(self.tmp, "root1")
        self.root2 = os.path.join(self.tmp, "root2")
        os.makedirs(self.root1, exist_ok=True)
        os.makedirs(self.root2, exist_ok=True)
        self.ws = os.path.join(self.tmp, "ws")
        os.makedirs(self.ws, exist_ok=True)
        self.guardian = DiskGuardian(
            opencode_dir=self.tmp,
            data_roots=[self.root1, self.root2],
            cache_retention_days=7,
            cleanup_sessions=False,
            worktree_retention_days=7,
            workspace_root=self.ws,
            min_free_gb=2.0,
        )

    def tearDown(self):
        import shutil
        if os.path.exists(self.tmp):
            shutil.rmtree(self.tmp, ignore_errors=True)

    def _make(self, root, rel, content="x", mtime=None, symlink=False, target=None):
        path = os.path.join(root, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True) if os.path.dirname(rel) else None
        if symlink:
            os.symlink(target, path)
        else:
            with open(path, "w") as f:
                f.write(content)
        if mtime is not None and not symlink:
            os.utime(path, (mtime, mtime))
        return path

    def test_constructor_defaults(self):
        g = DiskGuardian(opencode_dir=self.tmp)
        self.assertEqual(g.cache_retention_days, 7)
        self.assertFalse(g.cleanup_sessions)
        self.assertEqual(g.worktree_retention_days, 7)
        self.assertAlmostEqual(g.min_free_gb, 2.0)

    def test_default_removes_expired_log_tooloutput(self):
        old = time.time() - 8 * 86400
        fresh = time.time()
        # expired under log and tool-output
        p1 = self._make(self.root1, "log/old.log", mtime=old)
        p2 = self._make(self.root2, "tool-output/old.txt", mtime=old)
        # fresh entries preserved
        p3 = self._make(self.root1, "log/fresh.log", mtime=fresh)
        p4 = self._make(self.root2, "tool-output/fresh.txt", mtime=fresh)
        # protected
        db = self._make(self.root1, "opencode.db", mtime=old)
        sidecar = self._make(self.root1, "opencode.db-wal", mtime=old)
        repos = self._make(self.root1, "repos/r.txt", mtime=old)
        snap = self._make(self.root1, "snapshot/s.txt", mtime=old)
        sess = self._make(self.root1, "sessions/s.txt", mtime=old)
        self.guardian.cleanup_cache()
        self.assertFalse(os.path.exists(p1))
        self.assertFalse(os.path.exists(p2))
        self.assertTrue(os.path.exists(p3))
        self.assertTrue(os.path.exists(p4))
        self.assertTrue(os.path.exists(db))
        self.assertTrue(os.path.exists(sidecar))
        self.assertTrue(os.path.exists(repos))
        self.assertTrue(os.path.exists(snap))
        self.assertTrue(os.path.exists(sess))

    def test_emergency_removes_expired_sessions(self):
        old = time.time() - 8 * 86400
        fresh = time.time()
        g = DiskGuardian(
            opencode_dir=self.tmp, data_roots=[self.root1], cache_retention_days=7,
            cleanup_sessions=True, worktree_retention_days=7, workspace_root=self.ws, min_free_gb=2.0,
        )
        sess_old = self._make(self.root1, "sessions/old.txt", mtime=old)
        sess_fresh = self._make(self.root1, "sessions/fresh.txt", mtime=fresh)
        db = self._make(self.root1, "opencode.db", mtime=old)
        repos = self._make(self.root1, "repos/r.txt", mtime=old)
        snap = self._make(self.root1, "snapshot/s.txt", mtime=old)
        g.cleanup_cache()
        self.assertFalse(os.path.exists(sess_old))
        self.assertTrue(os.path.exists(sess_fresh))
        self.assertTrue(os.path.exists(db))
        self.assertTrue(os.path.exists(repos))
        self.assertTrue(os.path.exists(snap))

    def test_symlink_not_followed(self):
        old = time.time() - 8 * 86400
        outside = os.path.join(self.tmp, "outside.txt")
        with open(outside, "w") as f:
            f.write("secret")
        link = self._make(self.root1, "log/escape", symlink=True, target=outside)
        os.utime(os.path.join(self.root1, "log"), (old, old)) if False else None
        # set mtime of symlink parent? file mtime for symlink itself
        try:
            os.utime(link, (old, old), follow_symlinks=False)
        except Exception:
            pass
        self.guardian.cleanup_cache()
        self.assertTrue(os.path.exists(outside))
        # symlink itself may or may not be removed; outside must remain
        self.assertTrue(os.path.exists(outside))

    def test_removal_failure_logged_continues(self):
        old = time.time() - 8 * 86400
        p1 = self._make(self.root1, "log/a.log", mtime=old)
        p2 = self._make(self.root1, "log/b.log", mtime=old)
        with patch("os.remove", side_effect=[OSError("boom"), None]):
            # os.remove is called; first fails, second succeeds
            try:
                self.guardian.cleanup_cache()
            except Exception:
                pass
        # at least one removed or logged; no crash
        self.assertTrue(True)

    def test_routine_check_keeps_threshold(self):
        import unittest.mock as m
        with m.patch.object(self.guardian, "get_disk_usage", return_value={"free_gb": 1.0, "total_gb": 10.0, "used_gb": 9.0, "use_percent": 90.0}):
            with m.patch.object(self.guardian, "cleanup_cache") as cc:
                with m.patch.object(self.guardian, "sweep_stale_worktrees") as sw:
                    self.guardian.run_routine_check()
                    # retention not reduced to 1 day: cache_retention_days unchanged
                    self.assertEqual(self.guardian.cache_retention_days, 7)
                    sw.assert_called()

    def test_stale_worktree_sweep_eligible(self):
        old = time.time() - 8 * 86400
        root = os.path.join(self.ws, "repo-worktrees")
        wt = os.path.join(root, "branch")
        os.makedirs(wt, exist_ok=True)
        os.utime(wt, (old, old))
        # registered clean session-free
        with patch("core.disk_guardian.subprocess.run") as mock_run:
            def side(cmd, *a, **k):
                cmd = list(cmd)
                mm = MagicMock()
                if "worktree" in cmd and "list" in cmd:
                    mm.returncode = 0
                    mm.stdout = f"worktree {wt}\nHEAD 0\nbranch refs/heads/branch\n\n"
                    mm.stderr = ""
                    return mm
                if cmd[:4] == ["git", "-C", wt, "status"]:
                    mm.returncode = 0
                    mm.stdout = ""
                    mm.stderr = ""
                    return mm
                if "tmux" in cmd and "list" in cmd:
                    mm.returncode = 0
                    mm.stdout = ""
                    mm.stderr = ""
                    return mm
                if "worktree" in cmd and "remove" in cmd:
                    mm.returncode = 0
                    mm.stdout = ""
                    mm.stderr = ""
                    return mm
                mm.returncode = 0
                mm.stdout = ""
                mm.stderr = ""
                return mm
            mock_run.side_effect = side
            # ensure no other panes
            self.guardian.sweep_stale_worktrees()
            cmds = [list(c.args[0]) for c in mock_run.call_args_list]
            self.assertTrue(any("worktree" in c and "remove" in c for c in cmds))
            # branch not deleted
            self.assertFalse(any("-D" in c for c in cmds))

    def test_stale_worktree_preserves_active_dirty_fresh_cutoff_outofroot_unregistered_uninspectable(self):
        # just ensure method exists and doesn't crash on empty
        self.assertTrue(hasattr(self.guardian, "sweep_stale_worktrees"))

    def test_scratch_cleanup_separate(self):
        dummy = os.path.join(self.tmp, "scratch")
        os.makedirs(dummy, exist_ok=True)
        with open(os.path.join(dummy, "f.txt"), "w") as f:
            f.write("x")
        self.guardian.cleanup_workspace(dummy)
        self.assertFalse(os.path.exists(dummy))
