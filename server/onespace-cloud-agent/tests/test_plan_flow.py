#!/usr/bin/env python3
"""End-to-end integration flow for the cloud plan execution workflow."""

import base64
import io
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from agent import main as agent_main
from core.plan_execution import PlanExecutor
from core.plan_tasks import PlanTaskManager

import core.issue_worker as issue_worker_mod

# Reuse the scripted runner from existing tests by importing the module.
import importlib.util

_spec = importlib.util.spec_from_file_location("tpe", os.path.join(os.path.dirname(__file__), "test_plan_execution.py"))
_tpe = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_tpe)
ScriptedRunner = _tpe.ScriptedRunner
FakeServerStarter = _tpe.FakeServerStarter
FakeClient = _tpe.FakeClient
_make_executor = _tpe._make_executor


def _b64(data):
    if isinstance(data, str):
        data = data.encode()
    return base64.b64encode(data).decode()


def _git(cwd, *args):
    subprocess.run(["git", "-C", cwd, *args], check=True)


class TestPlanFlow(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="pf_")
        self.state_root = os.path.join(self.tmp, "state")
        os.makedirs(self.state_root)
        self.workspace_root = os.path.join(self.tmp, "ws")
        os.makedirs(self.workspace_root)

        # local bare repo
        self.repo = os.path.join(self.tmp, "repo.git")
        os.makedirs(self.repo)
        subprocess.run(["git", "init", "-q", "--bare", self.repo], check=True)
        wd = os.path.join(self.tmp, "wd")
        os.makedirs(wd)
        subprocess.run(["git", "clone", "-q", self.repo, wd], check=True)
        subprocess.run(["git", "-C", wd, "config", "user.email", "t@t.com"], check=True)
        subprocess.run(["git", "-C", wd, "config", "user.name", "t"], check=True)
        with open(os.path.join(wd, "README.md"), "w") as f:
            f.write("hi")
        subprocess.run(["git", "-C", wd, "add", "README.md"], check=True)
        subprocess.run(["git", "-C", wd, "commit", "-q", "-m", "init"], check=True)
        subprocess.run(["git", "-C", wd, "branch", "-M", "main"], check=True)
        subprocess.run(["git", "-C", wd, "push", "-q", "origin", "main"], check=True)
        self.latest = subprocess.check_output(["git", "-C", self.repo, "rev-parse", "main"]).decode().strip()

        self.gh_calls = []
        self.tmux_calls = []

        def gh_runner(cmd, *a, **k):
            self.gh_calls.append(list(cmd))
            return _tpe.SimpleNamespace(returncode=0, stdout=json.dumps({"nameWithOwner": "owner/repo"}), stderr="")

        def tmux_runner(cmd, *a, **k):
            self.tmux_calls.append(list(cmd))
            return _tpe.SimpleNamespace(returncode=0, stdout="", stderr="")

        self.gh_runner = gh_runner
        self.tmux_runner = tmux_runner
        self.config = {"plan_jobs": {"workspace_root": self.workspace_root},
                       "issue_worker": {"workspace_root": self.workspace_root}}
        self.clock = lambda: 1000.0
        self.mgr = PlanTaskManager(
            self.config, state_root=self.state_root, workspace_root=self.workspace_root,
            gh_runner=gh_runner, tmux_runner=tmux_runner, clock=self.clock,
        )

        # envelope
        binary = bytes(range(256)) * 2
        plan_id = "plan-int"
        self.plan_id = plan_id
        env = {
            "schema_version": 1,
            "request_id": "req-1",
            "workspace": {"id": "w1", "name": "ws", "root_path": "/tmp/ws"},
            "repository": {
                "remote_url": self.repo,
                "canonical": None,
                "host": "github.com",
                "branch": "main",
                "base_branch": "main",
                "source": "git",
                "head_sha": "abc",
                "dirty": False,
            },
            "plan": {
                "plan_id": plan_id,
                "title": "Plan Int",
                "directory": "plan",
                "files": [
                    {"path": "nested/a.txt", "content_base64": _b64("hello")},
                    {"path": "nested/bin.dat", "content_base64": _b64(binary)},
                ],
            },
            "policy": {"auto_permission_once": True, "auto_recommended_questions": True, "auto_merge": False},
        }
        self.envelope = env
        self.binary = binary

    def tearDown(self):
        import shutil
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_flow(self):
        # 2. submit
        raw = json.dumps(self.envelope).encode()
        r1 = self.mgr.submit(raw, "req-1")
        self.assertTrue(r1["ok"])
        job_id = r1["task"]["id"]
        self.assertEqual(len(self.tmux_calls), 1)
        # uploaded bytes preserved
        upload_root = Path(self.state_root) / job_id / "upload"
        self.assertEqual((upload_root / "nested" / "a.txt").read_text(), "hello")
        self.assertEqual((upload_root / "nested" / "bin.dat").read_bytes(), self.binary)
        # repeated submit new request id => reused
        env2 = json.loads(json.dumps(self.envelope))
        env2["request_id"] = "req-2"
        r2 = self.mgr.submit(env2, "req-2")
        self.assertTrue(r2["ok"])
        self.assertTrue(r2.get("reused"))
        self.assertEqual(r1["task"]["id"], r2["task"]["id"])

        # 3. prepare workspace
        import core.plan_execution as _pe
        _orig_ej = getattr(_pe, "execute_job", None)
        _pe.execute_job = None
        try:
            self.mgr.run_job(job_id)
        finally:
            if _orig_ej is not None:
                _pe.execute_job = _orig_ej
        task = self.mgr.inspect(job_id)
        remote_root = task["remote_root"]
        self.assertEqual(task["base_commit"], self.latest)
        plan_dest = Path(remote_root) / ".ai-workflow" / "plans" / self.plan_id
        self.assertTrue((plan_dest / "nested" / "a.txt").exists())
        self.assertEqual((plan_dest / "nested" / "bin.dat").read_bytes(), self.binary)

        # 4. simulate coding completion
        # commit source change on delivery branch
        os.makedirs(os.path.join(remote_root, ".ai-workflow"), exist_ok=True)
        with open(os.path.join(remote_root, "AGENTS.md"), "w") as f:
            f.write("# project contract\n")
        with open(os.path.join(remote_root, ".ai-workflow", "AGENTS.md"), "w") as f:
            f.write("# project contract\n")
        with open(os.path.join(remote_root, "changed.txt"), "w") as f:
            f.write("change")
        _git(remote_root, "add", "-A")
        _git(remote_root, "config", "user.email", "t@t.com")
        _git(remote_root, "config", "user.name", "t")
        _git(remote_root, "commit", "-q", "-m", "coding")
        final_sha = subprocess.check_output(["git", "-C", remote_root, "rev-parse", "HEAD"]).decode().strip()
        impl = plan_dest / "implementation.yaml"
        impl.write_text("status: completed\ncommit: %s\n" % final_sha)

        # run execute_job with fake server/client
        runner = ScriptedRunner(final_sha=final_sha, changed=True)
        server = FakeServerStarter()
        client = FakeClient(server.url, remote_root, events_batches=[[_tpe._skill_event(), {"type": "session.status", "properties": {"sessionID": "ses_parent", "status": {"type": "idle"}}}]])
        executor, factory_calls = _make_executor(self.mgr, client, runner, server)
        # monkeypatch execute_job to use our executor? execute_job creates PlanExecutor(manager) default. 
        # Instead call executor.execute with context built from manager.
        import core.plan_execution as pe
        context = pe.build_execution_context(self.mgr, self.mgr.inspect(job_id))
        # ensure plan_dir points to remote_root plan
        result = executor.execute(context)
        self.assertIsNotNone(result)
        final_task = self.mgr.inspect(job_id)
        self.assertEqual(final_task["status"], "completed")
        self.assertEqual(final_task["final_commit"], final_sha)
        self.assertEqual(final_task["pr_url"], "https://github.com/owner/repo/pull/7")
        # exactly one non-force push
        pushes = [c for c in runner.calls if c and c[0] == "git" and "push" in c]
        self.assertEqual(len(pushes), 1)
        self.assertNotIn("--force", pushes[0])
        # PR create with explicit repo/base/head and no Closes
        creates = [c for c in runner.calls if c and c[0] == "gh" and "create" in c and "pr" in c]
        self.assertEqual(len(creates), 1)
        joined = " ".join(creates[0])
        self.assertIn("--base", joined)
        self.assertIn("--head", joined)
        self.assertIn("--repo", joined)
        # check no Closes in PR body? runner doesn't capture body; inspect call args
        self.assertNotIn("Closes", joined)

        # 5. reconnect
        mgr2 = PlanTaskManager(
            self.config, state_root=self.state_root, workspace_root=self.workspace_root,
            gh_runner=self.gh_runner, tmux_runner=self.tmux_runner, clock=self.clock,
        )
        self.assertEqual(len(mgr2.list_tasks()), 1)
        t2 = mgr2.inspect(job_id)
        self.assertEqual(t2["status"], "completed")
        self.assertEqual(t2["pr_url"], "https://github.com/owner/repo/pull/7")

        # 6. cancel outcome second job
        env3 = json.loads(json.dumps(self.envelope))
        env3["request_id"] = "req-cancel"
        env3["plan"]["plan_id"] = "plan-cancel"
        r3 = self.mgr.submit(env3, "req-cancel")
        self.assertTrue(r3["ok"])
        cancel_job = r3["task"]["id"]
        c1 = self.mgr.cancel(cancel_job)
        self.assertTrue(c1["ok"])
        self.assertEqual(c1["task"]["status"], "cancelled")
        c2 = self.mgr.cancel(cancel_job)
        self.assertTrue(c2["ok"])
        self.assertEqual(c1["task"]["status"], c2["task"]["status"])
        # no push/pr for cancelled
        # workspace/upload preserved
        self.assertTrue((Path(self.state_root) / cancel_job / "upload").exists())

        # 7. agent tasks --json view
        with mock.patch("core.plan_tasks.DEFAULT_STATE_ROOT", self.state_root):
            with mock.patch.object(issue_worker_mod.IssueWorker, "read_task_history_file", return_value=[
                {"id": "i1", "source": "issue", "repo": "owner/repo", "issue_number": 1, "title": "Issue", "status": "completed"}
            ]):
                buf = io.StringIO()
                with mock.patch("core.plan_tasks.DEFAULT_STATE_ROOT", self.state_root):
                    rc = agent_main(["tasks", "--json", "--config", self._write_config()], stdout=buf)
                self.assertEqual(rc, 0)
                data = json.loads(buf.getvalue())
                plan_rows = [x for x in data if x.get("source") == "plan"]
                issue_rows = [x for x in data if x.get("source") != "plan"]
                self.assertGreaterEqual(len(plan_rows), 1)
                self.assertEqual(issue_rows[0]["issue_number"], 1)
                for pr in plan_rows:
                    self.assertIsNone(pr.get("issue_number"))

    def _write_config(self):
        path = os.path.join(self.tmp, "config.json")
        with open(path, "w") as f:
            json.dump(self.config, f)
        return path


if __name__ == "__main__":
    unittest.main()
