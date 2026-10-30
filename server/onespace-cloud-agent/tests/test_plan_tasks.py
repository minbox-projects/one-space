#!/usr/bin/env python3
"""Behavior tests for PlanTaskManager (RED until core/plan_tasks.py exists)."""

import base64
import json
import os
import subprocess
import tempfile
import threading
import time
import unittest
from pathlib import Path

from core.plan_tasks import PlanTaskManager, PlanError, SCHEMA_VERSION, TERMINAL_STATUSES


def _b64(s):
    return base64.b64encode(s.encode()).decode()


def _make_envelope(tmp, plan_id="p1", base="main", content="hello", path="a.txt"):
    plan_dir = os.path.join(tmp, "plan")
    os.makedirs(plan_dir, exist_ok=True)
    return {
        "schema_version": 1,
        "request_id": "req-1",
        "workspace": {"id": "w1", "name": "ws", "root_path": "/tmp/ws"},
        "repository": {
            "remote_url": "https://example.com/o/r.git",
            "canonical": None,
            "host": "github.com",
            "branch": "main",
            "base_branch": base,
            "source": "git",
            "head_sha": "abc",
            "dirty": False,
        },
        "plan": {
            "plan_id": plan_id,
            "title": "Plan",
            "directory": "plan",
            "files": [{"path": path, "content_base64": _b64(content)}],
        },
        "policy": {"auto_permission_once": True, "auto_recommended_questions": True, "auto_merge": False},
    }


class FakeProc:
    def __init__(self, returncode=0, stdout="", stderr=""):
        self.returncode = returncode
        self.stdout = stdout
        self.stderr = stderr


class TestPlanTasks(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="pt_")
        self.state_root = os.path.join(self.tmp, "state")
        self.workspace_root = os.path.join(self.tmp, "ws")
        os.makedirs(self.workspace_root)
        self.gh_calls = []
        self.tmux_calls = []
        self.tmux_has_session_rc = 0

        def gh_runner(cmd, *a, **k):
            self.gh_calls.append(list(cmd))
            # canonical identity
            return FakeProc(0, json.dumps({"nameWithOwner": "owner/repo"}), "")

        def tmux_runner(cmd, *a, **k):
            self.tmux_calls.append(list(cmd))
            if isinstance(cmd, (list, tuple)) and "has-session" in cmd:
                return FakeProc(self.tmux_has_session_rc, "", "")
            return FakeProc(0, "", "")

        self.gh_runner = gh_runner
        self.tmux_runner = tmux_runner
        self.config = {"plan_jobs": {"workspace_root": self.workspace_root},
                       "issue_worker": {"workspace_root": self.workspace_root}}
        self.clock = lambda: 1000.0
        self.mgr = PlanTaskManager(
            self.config, state_root=self.state_root, workspace_root=self.workspace_root,
            gh_runner=gh_runner, tmux_runner=tmux_runner, clock=self.clock,
        )

    def tearDown(self):
        import shutil
        shutil.rmtree(self.tmp, ignore_errors=True)

    # decode refusals
    def test_decode_absolute_path_raises(self):
        env = _make_envelope(self.tmp, path="/abs.txt")
        raw = json.dumps(env).encode()
        with self.assertRaises(PlanError):
            self.mgr.decode_submit_envelope(raw)

    def test_decode_traversal_raises(self):
        env = _make_envelope(self.tmp, path="../x.txt")
        raw = json.dumps(env).encode()
        with self.assertRaises(PlanError):
            self.mgr.decode_submit_envelope(raw)

    def test_decode_duplicate_raises(self):
        env = _make_envelope(self.tmp)
        env["plan"]["files"].append({"path": "a.txt", "content_base64": _b64("dup")})
        raw = json.dumps(env).encode()
        with self.assertRaises(PlanError):
            self.mgr.decode_submit_envelope(raw)

    def test_decode_bad_base64_raises(self):
        env = _make_envelope(self.tmp)
        env["plan"]["files"][0]["content_base64"] = "!!!notbase64"
        raw = json.dumps(env).encode()
        with self.assertRaises(PlanError):
            self.mgr.decode_submit_envelope(raw)

    def test_decode_invalid_json_raises(self):
        with self.assertRaises(PlanError):
            self.mgr.decode_submit_envelope(b"{not json")

    # canonical identity aliasing
    def test_canonical_identity_aliasing(self):
        env = _make_envelope(self.tmp)
        r1 = self.mgr.submit(env, "req-1")
        self.assertTrue(r1["ok"])
        # SSH spelling variant should resolve same identity via fake gh
        env2 = json.loads(json.dumps(env))
        env2["repository"]["remote_url"] = "git@github.com:owner/repo.git"
        env2["request_id"] = "req-2"
        r2 = self.mgr.submit(env2, "req-2")
        # same identity+bytes+base => reused
        self.assertTrue(r2["ok"])
        self.assertTrue(r2.get("reused"))

    # admission once + replay same/new request id
    def test_admission_replay_same_request(self):
        env = _make_envelope(self.tmp)
        r1 = self.mgr.submit(env, "req-1")
        self.assertTrue(r1["ok"])
        r2 = self.mgr.submit(env, "req-1")
        self.assertTrue(r2["ok"])
        self.assertTrue(r2["reused"])
        self.assertEqual(r1["task"]["id"], r2["task"]["id"])

    def test_admission_replay_new_request(self):
        env = _make_envelope(self.tmp)
        r1 = self.mgr.submit(env, "req-1")
        env2 = json.loads(json.dumps(env))
        env2["request_id"] = "req-2"
        r2 = self.mgr.submit(env2, "req-2")
        self.assertTrue(r2["ok"])
        self.assertTrue(r2["reused"])
        self.assertEqual(r1["task"]["id"], r2["task"]["id"])

    # conflicts
    def test_conflict_changed_base(self):
        env = _make_envelope(self.tmp, base="main")
        self.mgr.submit(env, "req-1")
        env2 = json.loads(json.dumps(env))
        env2["repository"]["base_branch"] = "dev"
        env2["request_id"] = "req-2"
        r = self.mgr.submit(env2, "req-2")
        self.assertFalse(r["ok"])
        self.assertEqual(r["code"], "identity_conflict")

    def test_conflict_changed_bytes(self):
        env = _make_envelope(self.tmp, content="hello")
        self.mgr.submit(env, "req-1")
        env2 = json.loads(json.dumps(env))
        env2["plan"]["files"][0]["content_base64"] = _b64("changed")
        env2["request_id"] = "req-2"
        r = self.mgr.submit(env2, "req-2")
        self.assertFalse(r["ok"])
        self.assertEqual(r["code"], "identity_conflict")

    def test_unresolved_identity(self):
        def gh_fail(cmd, *a, **k):
            return FakeProc(1, "", "not found")
        mgr = PlanTaskManager(
            self.config, state_root=os.path.join(self.tmp, "state2"),
            workspace_root=self.workspace_root, gh_runner=gh_fail,
            tmux_runner=self.tmux_runner, clock=self.clock,
        )
        env = _make_envelope(self.tmp)
        r = mgr.submit(env, "req-1")
        self.assertFalse(r["ok"])
        self.assertEqual(r["code"], "unresolved_identity")

    def test_invalid_payload(self):
        r = self.mgr.submit({"bad": True}, "req-1")
        self.assertFalse(r["ok"])
        self.assertEqual(r["code"], "invalid_payload")

    # concurrent submissions
    def test_concurrent_one_job_one_tmux(self):
        env = _make_envelope(self.tmp)
        results = []

        def worker():
            e = json.loads(json.dumps(env))
            results.append(self.mgr.submit(e, e["request_id"]))

        # use same request id to trigger race
        envs = []
        for i in range(5):
            e = json.loads(json.dumps(env))
            e["request_id"] = f"req-{i}"
            envs.append(e)

        threads = []
        for e in envs:
            t = threading.Thread(target=lambda x=e: results.append(self.mgr.submit(x, x["request_id"])))
            threads.append(t)
        for t in threads:
            t.start()
        for t in threads:
            t.join()
        jobs = {r["task"]["id"] for r in results if r["ok"]}
        self.assertEqual(len(jobs), 1)
        new_sessions = [c for c in self.tmux_calls if "new-session" in c]
        self.assertEqual(len(new_sessions), 1)

    # binary bytes preserved
    def test_binary_bytes_preserved(self):
        data = bytes(range(256)) * 4
        b64 = base64.b64encode(data).decode()
        env = _make_envelope(self.tmp)
        env["plan"]["files"][0]["content_base64"] = b64
        env["plan"]["files"][0]["path"] = "bin.dat"
        r = self.mgr.submit(env, "req-1")
        self.assertTrue(r["ok"])
        job_id = r["task"]["id"]
        upload_file = Path(self.state_root) / job_id / "upload" / "bin.dat"
        self.assertTrue(upload_file.exists())
        self.assertEqual(upload_file.read_bytes(), data)

    # immutable admission bytes after run_job
    def test_immutable_after_run_job(self):
        env = _make_envelope(self.tmp)
        # A missing local remote fails without any network access, so run_job
        # stops before delegation while the immutable upload must remain intact.
        env["repository"]["remote_url"] = os.path.join(self.tmp, "missing.git")
        r = self.mgr.submit(env, "req-1")
        job_id = r["task"]["id"]
        upload_file = Path(self.state_root) / job_id / "upload" / "a.txt"
        before = upload_file.read_bytes()
        try:
            self.mgr.run_job(job_id)
        except Exception:
            pass
        self.assertEqual(upload_file.read_bytes(), before)
        # simulate coding writing implementation.yaml into execution copy
        exec_copy = Path(self.workspace_root) / "plan-jobs" / job_id / "workspace" / ".ai-workflow" / "plans" / "p1"
        exec_copy.mkdir(parents=True, exist_ok=True)
        (exec_copy / "implementation.yaml").write_text("done: true\n")
        self.assertEqual(upload_file.read_bytes(), before)

    # state persists
    def test_state_persists(self):
        env = _make_envelope(self.tmp)
        r = self.mgr.submit(env, "req-1")
        job_id = r["task"]["id"]
        mgr2 = PlanTaskManager(
            self.config, state_root=self.state_root, workspace_root=self.workspace_root,
            gh_runner=self.gh_runner, tmux_runner=self.tmux_runner, clock=self.clock,
        )
        self.assertIsNotNone(mgr2.inspect(job_id))
        self.assertEqual(len(mgr2.list_tasks()), 1)

    def test_cancel_terminates_owned_server_pid_only(self):
        import subprocess
        import os
        import signal
        env = _make_envelope(self.tmp)
        r = self.mgr.submit(env, "req-1")
        job_id = r["task"]["id"]
        proc = subprocess.Popen(
            ["sleep", "60"], start_new_session=True
        )
        try:
            self.mgr.update(job_id, server_pid=proc.pid)
            result = self.mgr.cancel(job_id)
            self.assertTrue(result["ok"])
            deadline = time.time() + 2
            while time.time() < deadline and proc.poll() is None:
                time.sleep(0.1)
            self.assertIsNotNone(proc.poll())
            # workspace remains
            self.assertTrue(os.path.isdir(self.state_root))
            # no extra kill-session? controller kill still expected; assert at least no crash
        finally:
            if proc.poll() is None:
                try:
                    os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
                except ProcessLookupError:
                    pass

    def test_cancel_owned_only_and_repeat(self):
        env = _make_envelope(self.tmp)
        r = self.mgr.submit(env, "req-1")
        job_id = r["task"]["id"]
        c1 = self.mgr.cancel(job_id)
        self.assertTrue(c1["ok"])
        c2 = self.mgr.cancel(job_id)
        self.assertTrue(c2["ok"])
        self.assertEqual(c1["task"]["status"], c2["task"]["status"])
        # tmux kill-session called
        kills = [c for c in self.tmux_calls if "kill-session" in c]
        self.assertGreaterEqual(len(kills), 1)

    def test_cancel_terminal_unchanged(self):
        env = _make_envelope(self.tmp)
        r = self.mgr.submit(env, "req-1")
        job_id = r["task"]["id"]
        # mark terminal by direct state? Use cancel first
        self.mgr.cancel(job_id)
        t_before = self.mgr.inspect(job_id)
        c = self.mgr.cancel(job_id)
        self.assertEqual(c["task"]["status"], t_before["status"])

    # interrupted controller detection
    def test_interrupted_controller_marks_failed(self):
        env = _make_envelope(self.tmp)
        r = self.mgr.submit(env, "req-1")
        job_id = r["task"]["id"]
        self.tmux_has_session_rc = 1
        tasks = self.mgr.list_tasks()
        t = [x for x in tasks if x["id"] == job_id][0]
        self.assertEqual(t["status"], "failed")
        # no new-session rerun
        new_sessions = [c for c in self.tmux_calls if "new-session" in c]
        self.assertEqual(len(new_sessions), 1)

    # aggregate
    def test_aggregate_sorting_and_issue_compat(self):
        env = _make_envelope(self.tmp)
        r = self.mgr.submit(env, "req-1")
        issue_tasks = [{"id": "i1", "source": "issue", "repo": "o/r", "issue_number": 1, "title": "Issue", "status": "completed"}]
        agg = self.mgr.aggregate_tasks(issue_tasks)
        self.assertEqual(len(agg), 2)
        plan_rows = [x for x in agg if x.get("source") == "plan"]
        issue_rows = [x for x in agg if x.get("source") != "plan"]
        self.assertEqual(len(plan_rows), 1)
        self.assertEqual(issue_rows[0], issue_tasks[0])

    def test_projection_keys(self):
        env = _make_envelope(self.tmp)
        r = self.mgr.submit(env, "req-1")
        task = r["task"]
        expected = {"id", "source", "repo", "issue_number", "title", "status", "started_at", "finished_at",
                    "duration_seconds", "branch", "pr_url", "error_message", "author", "trigger_type", "attempts",
                    "max_attempts", "diff_stat", "step", "plan_id", "plan_title", "workspace_id", "base_branch",
                    "base_commit", "final_commit", "session_id", "session_url", "stage", "question", "decision",
                    "remote_root", "local_root", "updated_at"}
        self.assertTrue(expected.issubset(set(task.keys())))
        self.assertEqual(task["source"], "plan")
        self.assertIsNone(task["issue_number"])
        self.assertEqual(task["trigger_type"], "plan")


class TestRunJobDelegates(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="pt2_")
        self.state_root = os.path.join(self.tmp, "state")
        self.workspace_root = os.path.join(self.tmp, "ws")
        os.makedirs(self.workspace_root)
        # local bare repo
        self.repo = os.path.join(self.tmp, "repo.git")
        os.makedirs(self.repo)
        subprocess.run(["git", "init", "-q", "--bare", self.repo], check=True)
        # seed commit on main
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

        self.gh_calls = []
        self.tmux_calls = []

        def gh_runner(cmd, *a, **k):
            self.gh_calls.append(list(cmd))
            return FakeProc(0, json.dumps({"nameWithOwner": "owner/repo"}), "")

        def tmux_runner(cmd, *a, **k):
            self.tmux_calls.append(list(cmd))
            return FakeProc(0, "", "")

        self.gh_runner = gh_runner
        self.tmux_runner = tmux_runner
        self.config = {"plan_jobs": {"workspace_root": self.workspace_root},
                       "issue_worker": {"workspace_root": self.workspace_root}}
        self.clock = lambda: 1000.0
        self.mgr = PlanTaskManager(
            self.config, state_root=self.state_root, workspace_root=self.workspace_root,
            gh_runner=gh_runner, tmux_runner=tmux_runner, clock=self.clock,
        )

    def tearDown(self):
        import shutil
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_run_job_delegates_to_execute_job(self):
        env = _make_envelope(self.tmp)
        env["repository"]["remote_url"] = self.repo
        env["repository"]["base_branch"] = "main"
        r = self.mgr.submit(env, "req-1")
        self.assertTrue(r["ok"])
        job_id = r["task"]["id"]
        upload_file = Path(self.state_root) / job_id / "upload" / "a.txt"
        before = upload_file.read_bytes()
        latest = subprocess.check_output(
            ["git", "-C", self.repo, "rev-parse", "main"]
        ).decode().strip()

        import core.plan_execution as pe
        calls = []

        def fake_execute(manager, jid):
            calls.append((manager, jid))
            return {"ok": True}

        orig = getattr(pe, "execute_job", None)
        pe.execute_job = fake_execute
        try:
            self.mgr.run_job(job_id)
        finally:
            if orig is not None:
                pe.execute_job = orig
            else:
                delattr(pe, "execute_job")

        self.assertEqual(calls, [(self.mgr, job_id)])
        prepared = self.mgr.inspect(job_id)
        self.assertIsNotNone(prepared["remote_root"])
        self.assertTrue(prepared["remote_root"].startswith(self.workspace_root))
        self.assertEqual(prepared["base_commit"], latest)
        self.assertEqual(prepared["branch"], "ai-workflow/%s" % job_id)
        self.assertEqual(upload_file.read_bytes(), before)

    def test_atomic_update_and_get_request_and_record_decision(self):
        env = _make_envelope(self.tmp)
        env["repository"]["remote_url"] = self.repo
        env["repository"]["base_branch"] = "main"
        r = self.mgr.submit(env, "req-1")
        job_id = r["task"]["id"]
        # atomic update
        updated = self.mgr.update(job_id, status="running", stage="coding")
        self.assertEqual(updated["status"], "running")
        self.assertEqual(self.mgr.inspect(job_id)["status"], "running")
        # get_request
        req = self.mgr.get_request(job_id)
        self.assertIsInstance(req, dict)
        # record_decision
        self.mgr.record_decision(job_id, {"q": "x"}, ["label"], "marker")
        rec = self.mgr._read_record(job_id)
        self.assertIsNotNone(rec.get("decision") or rec.get("decisions"))


if __name__ == "__main__":
    unittest.main()
