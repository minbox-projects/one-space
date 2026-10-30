#!/usr/bin/env python3
"""Behavior tests for plan_workspace (RED until core/plan_workspace.py exists)."""

import os
import re
import subprocess
import tempfile
import unittest

from core.plan_workspace import prepare_workspace, WorkspaceError


def _git(cwd, *args):
    return subprocess.run(
        ["git", *args], cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        text=True, check=True,
    )


class TestPlanWorkspace(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="pw_")
        self.workspace_root = os.path.join(self.tmp, "workspaces")
        os.makedirs(self.workspace_root)

    def _make_bare(self, name="repo.git"):
        bare = os.path.join(self.tmp, name)
        os.makedirs(bare)
        _git(bare, "init", "--bare")
        # seed a commit on main
        seed = os.path.join(self.tmp, name + "-seed")
        os.makedirs(seed)
        _git(seed, "init")
        _git(seed, "config", "user.email", "t@t.com")
        _git(seed, "config", "user.name", "t")
        with open(os.path.join(seed, "README.md"), "w") as f:
            f.write("hello\n")
        _git(seed, "add", "README.md")
        _git(seed, "commit", "-m", "init")
        _git(seed, "branch", "-M", "main")
        _git(seed, "remote", "add", "origin", bare)
        _git(seed, "push", "-u", "origin", "main")
        return bare, seed

    def test_fresh_clone_uses_latest_sha(self):
        bare, seed = self._make_bare()
        # advance remote
        with open(os.path.join(seed, "README.md"), "w") as f:
            f.write("v2\n")
        _git(seed, "add", "README.md")
        _git(seed, "commit", "-m", "v2")
        _git(seed, "push", "origin", "main")
        latest = _git(seed, "rev-parse", "HEAD").stdout.strip()

        task = {"id": "job-1", "repository": {"remote_url": bare, "base_branch": "main"}}
        res = prepare_workspace(task, workspace_root=self.workspace_root)
        self.assertEqual(len(res["base_commit"]), 40)
        self.assertRegex(res["base_commit"], r"^[0-9a-f]{40}$")
        self.assertEqual(res["base_commit"], latest)
        self.assertTrue(os.path.isdir(os.path.join(self.workspace_root, "plan-jobs", "job-1", "workspace", ".git")))

    def test_same_basename_different_remotes_distinct(self):
        bare1, _ = self._make_bare("a.git")
        bare2, _ = self._make_bare("b.git")
        # rename dirs to same basename by using parent? Instead just two jobs
        t1 = {"id": "job-a", "repository": {"remote_url": bare1, "base_branch": "main"}}
        t2 = {"id": "job-b", "repository": {"remote_url": bare2, "base_branch": "main"}}
        r1 = prepare_workspace(t1, workspace_root=self.workspace_root)
        r2 = prepare_workspace(t2, workspace_root=self.workspace_root)
        self.assertNotEqual(
            os.path.join(self.workspace_root, "plan-jobs", "job-a", "workspace"),
            os.path.join(self.workspace_root, "plan-jobs", "job-b", "workspace"),
        )

    def test_clone_failure_raises(self):
        task = {"id": "job-x", "repository": {"remote_url": "/no/such/repo.git", "base_branch": "main"}}
        with self.assertRaises(WorkspaceError):
            prepare_workspace(task, workspace_root=self.workspace_root)

    def test_fetch_failure_raises(self):
        bare, _ = self._make_bare()
        task = {"id": "job-y", "repository": {"remote_url": bare, "base_branch": "nope-branch"}}
        with self.assertRaises(WorkspaceError):
            prepare_workspace(task, workspace_root=self.workspace_root)

    def test_no_gitmodules_no_submodule_init(self):
        bare, _ = self._make_bare()
        # inject a fake runner that delegates to real git but records commands
        recorded = []

        def runner(cmd, *a, **k):
            recorded.append(list(cmd))
            return subprocess.run(cmd, *a, **k)

        task = {"id": "job-z", "repository": {"remote_url": bare, "base_branch": "main"}}
        prepare_workspace(task, workspace_root=self.workspace_root, runner=runner)
        joined = [" ".join(c) for c in recorded]
        self.assertFalse(any("submodule" in c for c in joined))

    def test_delivery_branch_per_job(self):
        bare, _ = self._make_bare()
        t1 = {"id": "job-1", "repository": {"remote_url": bare, "base_branch": "main"}}
        t2 = {"id": "job-2", "repository": {"remote_url": bare, "base_branch": "main"}}
        r1 = prepare_workspace(t1, workspace_root=self.workspace_root)
        r2 = prepare_workspace(t2, workspace_root=self.workspace_root)
        self.assertNotEqual(r1["delivery_branch"], r2["delivery_branch"])

    def test_gitlink_recorded_when_no_gitmodules(self):
        # smoke: base_commit remains 40-char even for repos without submodules
        bare, _ = self._make_bare()
        task = {"id": "job-g", "repository": {"remote_url": bare, "base_branch": "main"}}
        res = prepare_workspace(task, workspace_root=self.workspace_root)
        self.assertEqual(len(res["base_commit"]), 40)


if __name__ == "__main__":
    unittest.main()

