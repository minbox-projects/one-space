#!/usr/bin/env python3
"""Behavior tests for core/plan_execution.py (RED until module exists).

The executor contract exercised here is:

``PlanExecutor(manager, *, client_factory=None, runner=None, opener=None,
server_starter=None, clock=time.time, sleep=time.sleep,
max_wait_seconds=3600)``

``runner(cmd, **kwargs)`` is used for every subprocess, ``server_starter``
starts one owned loopback OpenCode server and returns a dict with ``url``,
``process`` and ``stop``, and ``client_factory(url, cloud_root)`` returns an
OpenCode client following the v1 session/permission/question protocol.

``PlanExecutor.preflight(context)`` validates prerequisites without starting a
server.  ``PlanExecutor.execute(context)`` drives one job until it is waiting
for a manual answer or reaches a terminal state, keeps its owned server live
while waiting, and returns the durable projection.
"""

import json
import os
import subprocess
import tempfile
import time
import unittest
from types import SimpleNamespace

from core.plan_execution import (
    PlanExecutionError,
    PlanExecutor,
    build_execution_context,
    build_execution_prompt,
    completion_evidence,
    map_command_paths,
    map_project_path,
    unmapped_paths,
)


BASE_SHA = "a" * 40
FINAL_SHA = "b" * 40
PR_URL = "https://github.com/owner/repo/pull/7"


def _ctx(tmp):
    cloud_root = os.path.join(tmp, "cloud")
    local_root = os.path.join(tmp, "local")
    os.makedirs(cloud_root, exist_ok=True)
    os.makedirs(local_root, exist_ok=True)
    return {
        "job_id": "job-1",
        "plan_dir": os.path.join(cloud_root, ".ai-workflow", "plans", "p1"),
        "cloud_root": cloud_root,
        "local_root": local_root,
        "delivery_branch": "ai-workflow/job-1",
        "base_branch": "main",
        "base_commit": BASE_SHA,
        "remote_url": "https://example.com/o/r.git",
        "canonical_repo": "owner/repo",
        "schedule_text": "- task-1",
        "policy": {"auto_permission_once": True, "auto_recommended_questions": True, "auto_merge": False},
    }


def _exec_ctx(tmp, *, job_id="job-1", plan_id="p1", base_commit=BASE_SHA,
              final_sha=FINAL_SHA, contract=True):
    cloud_root = os.path.join(tmp, "cloud")
    local_root = os.path.join(tmp, "local")
    plan_dir = os.path.join(cloud_root, ".ai-workflow", "plans", plan_id)
    os.makedirs(plan_dir, exist_ok=True)
    os.makedirs(local_root, exist_ok=True)
    if contract:
        with open(os.path.join(cloud_root, "AGENTS.md"), "w") as handle:
            handle.write("# project contract\n")
        with open(os.path.join(cloud_root, ".ai-workflow", "AGENTS.md"), "w") as handle:
            handle.write("# project contract\n")
    with open(os.path.join(plan_dir, "spec.md"), "w") as handle:
        handle.write("# spec\n")
    with open(os.path.join(plan_dir, "plan.md"), "w") as handle:
        handle.write("# plan\n")
    return {
        "job_id": job_id,
        "plan_id": plan_id,
        "plan_dir": plan_dir,
        "cloud_root": cloud_root,
        "local_root": local_root,
        "delivery_branch": "ai-workflow/%s" % job_id,
        "base_branch": "main",
        "base_commit": base_commit,
        "final_sha": final_sha,
        "remote_url": "https://example.com/o/r.git",
        "canonical_repo": "owner/repo",
        "schedule_text": "- task-1",
        "policy": {"auto_permission_once": True, "auto_recommended_questions": True, "auto_merge": False},
    }


def _write_record(context, *, status="completed", commit=None):
    plan_dir = context["plan_dir"]
    os.makedirs(plan_dir, exist_ok=True)
    path = os.path.join(plan_dir, "implementation.yaml")
    with open(path, "w") as handle:
        handle.write("status: %s\ncommit: %s\n" % (status, commit or context["final_sha"]))
    return path


def _skill_event():
    return {
        "type": "message.part.updated",
        "properties": {
            "part": {
                "type": "tool",
                "tool": "skill",
                "state": {"status": "completed", "input": {"name": "coding"}},
            }
        },
    }


def _idle_event(session_id="ses_parent"):
    return {"type": "session.idle", "properties": {"sessionID": session_id}}


def _permission(request_id, session_id):
    return {
        "id": request_id,
        "requestID": request_id,
        "sessionID": session_id,
        "permission": "bash",
        "patterns": ["git status"],
    }


def _permission_event(request_id, session_id):
    return {
        "type": "permission.asked",
        "properties": dict(_permission(request_id, session_id)),
    }


def _question(request_id, session_id, text, options):
    return {
        "id": request_id,
        "requestID": request_id,
        "sessionID": session_id,
        "questions": [
            {"question": text, "multiple": False, "options": options}
        ],
    }


def _question_event(request_id, session_id):
    return {
        "type": "question.asked",
        "properties": {"requestID": request_id, "sessionID": session_id},
    }


class FakeManager:
    """Small manager double with the executor-facing persistence surface."""

    def __init__(self, job_id="job-1", *, status="running", base_commit=BASE_SHA):
        self.base_commit = base_commit
        self.decisions = []
        self.requests = {job_id: {"plan_id": "p1"}}
        self.record = {
            "id": job_id,
            "status": status,
            "stage": "coding",
            "base_commit": base_commit,
            "final_commit": None,
            "pr_url": None,
            "question": None,
            "session_id": None,
            "session_url": None,
        }

    def inspect(self, job_id):
        if job_id != self.record["id"]:
            return None
        return dict(self.record)

    def update(self, job_id, **fields):
        if job_id != self.record["id"]:
            return None
        self.record.update(fields)
        return dict(self.record)

    def get_request(self, job_id):
        return dict(self.requests.get(job_id, {}))

    def record_decision(self, job_id, question, answer, basis):
        self.decisions.append(
            {"job_id": job_id, "question": question, "answer": answer, "basis": basis}
        )
        return True


class ScriptedRunner:
    """Deterministic subprocess runner covering every command the executor uses."""

    def __init__(
        self,
        *,
        base_commit=BASE_SHA,
        final_sha=FINAL_SHA,
        changed=True,
        push_rc=0,
        pr_list=None,
        pr_list_after_create=None,
        create_rc=0,
        create_url=PR_URL,
        ai_workflow_rc=0,
        opencode_rc=0,
        gh_auth_rc=0,
        skill_text="coding\n",
    ):
        self.base_commit = base_commit
        self.final_sha = final_sha
        self.changed = changed
        self.push_rc = push_rc
        self.pr_list_results = list(pr_list if pr_list is not None else [[]])
        self.pr_list_after_create = pr_list_after_create
        self.create_rc = create_rc
        self.create_url = create_url
        self.ai_workflow_rc = ai_workflow_rc
        self.opencode_rc = opencode_rc
        self.gh_auth_rc = gh_auth_rc
        self.skill_text = skill_text
        self.calls = []
        self._list_index = 0
        self.create_attempts = 0

    def _proc(self, returncode=0, stdout="", stderr=""):
        return SimpleNamespace(returncode=returncode, stdout=stdout, stderr=stderr)

    def _next_pr_list(self):
        if not self.pr_list_results:
            return []
        index = min(self._list_index, len(self.pr_list_results) - 1)
        self._list_index += 1
        return self.pr_list_results[index]

    def __call__(self, cmd, *args, **kwargs):
        command = list(cmd)
        self.calls.append(command)
        first = command[0] if command else ""

        if first == "ai-workflow":
            return self._proc(self.ai_workflow_rc, "ai-workflow 1.0.0\n", "" if not self.ai_workflow_rc else "missing")
        if first == "opencode":
            if "debug" in command:
                return self._proc(self.opencode_rc, self.skill_text, "" if not self.opencode_rc else "opencode failed")
            return self._proc(self.opencode_rc, "1.19.5\n", "" if not self.opencode_rc else "opencode failed")
        if first == "gh":
            if "auth" in command:
                return self._proc(self.gh_auth_rc, "logged in\n", "" if not self.gh_auth_rc else "not logged in")
            if "list" in command and "pr" in command:
                if self.pr_list_after_create is not None and self.create_attempts:
                    return self._proc(0, json.dumps(self.pr_list_after_create), "")
                return self._proc(0, json.dumps(self._next_pr_list()), "")
            if "create" in command and "pr" in command:
                self.create_attempts += 1
                if self.create_rc != 0:
                    return self._proc(self.create_rc, "", "create failed")
                url = self.create_url or ""
                return self._proc(0, (url + "\n") if url else "", "")
            return self._proc(0, "", "")

        if first == "git":
            if "status" in command:
                return self._proc(0, "", "")
            if "rev-parse" in command:
                return self._proc(0, self.final_sha + "\n", "")
            if "diff" in command:
                if "--quiet" in command:
                    return self._proc(1 if self.changed else 0, "", "")
                return self._proc(0, ("changed.txt\n" if self.changed else ""), "")
            if "push" in command:
                return self._proc(self.push_rc, "", "" if not self.push_rc else "push rejected")
            return self._proc(0, "", "")

        return self._proc(0, "", "")


class FakeServerStarter:
    def __init__(self, url="http://127.0.0.1:4096"):
        self.url = url
        self.contexts = []
        self.stops = 0

    def __call__(self, context):
        self.contexts.append(dict(context))

        def stop():
            self.stops += 1

        return {"url": self.url, "process": object(), "stop": stop}


class FakeClient:
    def __init__(
        self,
        url,
        cloud_root,
        *,
        doc_paths=None,
        events_batches=None,
        permissions=None,
        questions=None,
        session_id="ses_parent",
        child_id="ses_child",
    ):
        self.url = url
        self.cloud_root = cloud_root
        self.session_id = session_id
        self.child_id = child_id
        self.doc_paths = (
            dict(doc_paths)
            if doc_paths is not None
            else {"/session": {}, "/permission": {}, "/question": {}}
        )
        self.events_batches = list(events_batches or [])
        self.pending_permissions = list(permissions or [])
        self.pending_questions = list(questions or [])
        self.status = {session_id: {"type": "idle"}}
        self.after_stream = None
        self.created_sessions = []
        self.prompt_calls = []
        self.permission_replies = []
        self.question_replies = []
        self.stream_calls = 0

    def doc(self):
        return {"paths": dict(self.doc_paths)}

    def create_session(self, *, title=None, parent_id=None):
        self.created_sessions.append({"title": title, "parent_id": parent_id})
        return {"id": self.session_id}

    def prompt_async(self, session_id, text, *, agent=None, model=None):
        self.prompt_calls.append(
            {"session_id": session_id, "text": text, "agent": agent, "model": model}
        )
        return {"accepted": True}

    def children(self, session_id):
        return [
            {"id": self.session_id},
            {"id": self.child_id, "parentID": self.session_id},
        ]

    def session_status(self):
        return {key: dict(value) for key, value in self.status.items()}

    def permissions(self):
        return [dict(item) for item in self.pending_permissions]

    def questions(self):
        return [dict(item) for item in self.pending_questions]

    def reply_permission(self, request_id, reply="once"):
        self.permission_replies.append({"id": request_id, "reply": reply})
        self.pending_permissions = [
            item
            for item in self.pending_permissions
            if item.get("id") != request_id and item.get("requestID") != request_id
        ]
        return {"ok": True}

    def reply_question(self, request_id, answers):
        self.question_replies.append({"id": request_id, "answers": answers})
        self.pending_questions = [
            item
            for item in self.pending_questions
            if item.get("id") != request_id and item.get("requestID") != request_id
        ]
        return {"ok": True}

    def stream_events(self, stop_event, on_event):
        self.stream_calls += 1
        batch = self.events_batches.pop(0) if self.events_batches else []
        for event in batch:
            on_event(event)
        if self.after_stream is not None:
            self.after_stream(self)
        return None


def _make_executor(manager, client, runner, server):
    factory_calls = []

    def factory(url, cloud_root):
        factory_calls.append((url, cloud_root))
        return client

    executor = PlanExecutor(
        manager,
        client_factory=factory,
        runner=runner,
        opener=None,
        server_starter=server,
        clock=lambda: 1000.0,
        sleep=lambda _seconds: None,
        max_wait_seconds=5,
    )
    return executor, factory_calls


def _git_calls(runner, *words):
    return [
        call
        for call in runner.calls
        if call
        and call[0] == "git"
        and all(word in call for word in words)
    ]


def _gh_calls(runner, *words):
    return [
        call
        for call in runner.calls
        if call
        and call[0] == "gh"
        and all(word in call for word in words)
    ]


class TestContextAndMapping(unittest.TestCase):
    def test_build_execution_context_keys(self):
        class Mgr:
            pass

        task = {
            "id": "job-1",
            "plan_id": "p1",
            "base_branch": "main",
            "base_commit": BASE_SHA,
            "remote_url": None,
            "repository": {"remote_url": "https://example.com/o/r.git"},
            "local_root": "/tmp/local",
            "cloud_root": "/tmp/cloud",
        }
        ctx = build_execution_context(Mgr(), task)
        for key in (
            "job_id",
            "plan_dir",
            "cloud_root",
            "local_root",
            "delivery_branch",
            "base_branch",
            "base_commit",
            "remote_url",
            "canonical_repo",
        ):
            self.assertIn(key, ctx)

    def test_prompt_names(self):
        tmp = tempfile.mkdtemp()
        ctx = _ctx(tmp)
        prompt = build_execution_prompt(ctx)
        self.assertIn(ctx["plan_dir"], prompt)
        self.assertIn(ctx["delivery_branch"], prompt)
        self.assertIn("coding", prompt.lower())

    def test_map_project_path(self):
        tmp = tempfile.mkdtemp()
        ctx = _ctx(tmp)
        local = ctx["local_root"]
        cloud = ctx["cloud_root"]
        path = local + "/src/x.py"
        self.assertEqual(map_project_path(ctx, path), cloud + "/src/x.py")

    def test_map_command_paths(self):
        tmp = tempfile.mkdtemp()
        ctx = _ctx(tmp)
        local = ctx["local_root"]
        cloud = ctx["cloud_root"]
        command = "pytest " + local + "/tests"
        self.assertEqual(map_command_paths(ctx, command), "pytest " + cloud + "/tests")

    def test_unmapped_external(self):
        tmp = tempfile.mkdtemp()
        ctx = _ctx(tmp)
        external = "/etc/passwd"
        self.assertEqual(unmapped_paths(ctx, [external]), [external])
        self.assertEqual(unmapped_paths(ctx, ["src/x.py"]), [])


class TestCompletionEvidence(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.cloud = os.path.join(self.tmp, "cloud")
        os.makedirs(self.cloud)
        subprocess.run(["git", "init", "-q", self.cloud], check=True)
        subprocess.run(["git", "-C", self.cloud, "config", "user.email", "t@t.com"], check=True)
        subprocess.run(["git", "-C", self.cloud, "config", "user.name", "t"], check=True)
        with open(os.path.join(self.cloud, "f.txt"), "w") as handle:
            handle.write("x")
        subprocess.run(["git", "-C", self.cloud, "add", "f.txt"], check=True)
        subprocess.run(["git", "-C", self.cloud, "commit", "-q", "-m", "init"], check=True)
        self.base = subprocess.check_output(
            ["git", "-C", self.cloud, "rev-parse", "HEAD"]
        ).decode().strip()

    def _make_record(self, status="completed", commit=None):
        if commit is None:
            commit = subprocess.check_output(
                ["git", "-C", self.cloud, "rev-parse", "HEAD"]
            ).decode().strip()
        plan_dir = os.path.join(self.cloud, ".ai-workflow", "plans", "p1")
        os.makedirs(plan_dir, exist_ok=True)
        with open(os.path.join(plan_dir, "implementation.yaml"), "w") as handle:
            handle.write("status: %s\ncommit: %s\n" % (status, commit))
        return plan_dir, commit

    def _evidence(self, skill=True, record_path=None):
        ctx = _ctx(self.tmp)
        ctx["cloud_root"] = self.cloud
        ctx["base_commit"] = self.base
        ctx["plan_dir"] = os.path.join(self.cloud, ".ai-workflow", "plans", "p1")
        if record_path is None:
            record_path = os.path.join(ctx["plan_dir"], "implementation.yaml")
        return completion_evidence(
            ctx,
            observed_skill=skill,
            record_path=record_path,
            git_runner=subprocess.run,
            clock=time.time,
        )

    def test_skill_missing_invalid(self):
        plan_dir, _commit = self._make_record()
        evidence = self._evidence(skill=False, record_path=os.path.join(plan_dir, "implementation.yaml"))
        self.assertFalse(evidence["ok"])

    def test_record_missing_invalid(self):
        self._make_record()
        evidence = self._evidence(record_path="/no/such.yaml")
        self.assertFalse(evidence["ok"])

    def test_malformed_invalid(self):
        plan_dir, _commit = self._make_record()
        with open(os.path.join(plan_dir, "implementation.yaml"), "w") as handle:
            handle.write("not: [yaml")
        evidence = self._evidence(record_path=os.path.join(plan_dir, "implementation.yaml"))
        self.assertFalse(evidence["ok"])

    def test_not_completed_invalid(self):
        plan_dir, _commit = self._make_record(status="in-progress")
        evidence = self._evidence(record_path=os.path.join(plan_dir, "implementation.yaml"))
        self.assertFalse(evidence["ok"])

    def test_short_sha_invalid(self):
        plan_dir, _commit = self._make_record(commit="abc")
        evidence = self._evidence(record_path=os.path.join(plan_dir, "implementation.yaml"))
        self.assertFalse(evidence["ok"])

    def test_mismatched_sha_invalid(self):
        plan_dir, _commit = self._make_record(commit="c" * 40)
        evidence = self._evidence(record_path=os.path.join(plan_dir, "implementation.yaml"))
        self.assertFalse(evidence["ok"])

    def test_dirty_invalid(self):
        plan_dir, _commit = self._make_record()
        with open(os.path.join(self.cloud, "dirty.txt"), "w") as handle:
            handle.write("d")
        evidence = self._evidence(record_path=os.path.join(plan_dir, "implementation.yaml"))
        self.assertFalse(evidence["ok"])

    def test_clean_matching_valid(self):
        plan_dir, commit = self._make_record()
        evidence = self._evidence(record_path=os.path.join(plan_dir, "implementation.yaml"))
        self.assertTrue(evidence["ok"])
        self.assertEqual(evidence["final_commit"], commit)
        self.assertFalse(evidence["changed"])

    def test_changed_detection(self):
        with open(os.path.join(self.cloud, "f.txt"), "w") as handle:
            handle.write("changed")
        subprocess.run(["git", "-C", self.cloud, "add", "f.txt"], check=True)
        subprocess.run(["git", "-C", self.cloud, "commit", "-q", "-m", "c2"], check=True)
        head = subprocess.check_output(
            ["git", "-C", self.cloud, "rev-parse", "HEAD"]
        ).decode().strip()
        plan_dir, _commit = self._make_record(commit=head)
        evidence = self._evidence(record_path=os.path.join(plan_dir, "implementation.yaml"))
        self.assertTrue(evidence["ok"])
        self.assertTrue(evidence["changed"])


class TestPreflight(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="preflight_")

    def _executor(self, runner, client=None):
        manager = FakeManager()
        server = FakeServerStarter()
        client = client or FakeClient(server.url, self.tmp)
        executor, factory_calls = _make_executor(manager, client, runner, server)
        return executor, manager, server, factory_calls

    def test_all_prerequisites_pass_without_starting_server(self):
        ctx = _exec_ctx(self.tmp)
        runner = ScriptedRunner()
        executor, _manager, server, _factory_calls = self._executor(runner)
        executor.preflight(ctx)
        self.assertEqual(server.contexts, [])
        joined = [" ".join(call) for call in runner.calls]
        self.assertTrue(any(call.startswith("ai-workflow --version") for call in joined))
        self.assertTrue(any(call.startswith("opencode --version") for call in joined))
        self.assertTrue(any(call.startswith("opencode debug skill") for call in joined))
        self.assertTrue(any(call.startswith("gh auth status") for call in joined))

    def test_missing_contract_raises_distinct_stage(self):
        ctx = _exec_ctx(self.tmp, contract=False)
        runner = ScriptedRunner()
        executor, _manager, server, _factory_calls = self._executor(runner)
        with self.assertRaises(PlanExecutionError) as raised:
            executor.preflight(ctx)
        self.assertTrue(raised.exception.stage)
        self.assertEqual(server.contexts, [])

    def test_failing_prerequisites_raise_distinct_stages(self):
        cases = [
            ("workflow", {"ai_workflow_rc": 1}),
            ("opencode", {"opencode_rc": 1}),
            ("github", {"gh_auth_rc": 1}),
            ("skill", {"skill_text": "review\n"}),
            ("contract", None),
        ]
        stages = []
        for label, options in cases:
            with self.subTest(prerequisite=label):
                tmp = tempfile.mkdtemp(prefix="preflight_case_")
                ctx = _exec_ctx(tmp, contract=options is not None)
                runner = ScriptedRunner(**(options or {}))
                executor, _manager, server, _factory_calls = self._executor(runner)
                with self.assertRaises(PlanExecutionError) as raised:
                    executor.preflight(ctx)
                stages.append(raised.exception.stage)
                self.assertEqual(server.contexts, [])
        self.assertTrue(all(stages))
        self.assertEqual(len(set(stages)), len(cases))


class TestCapabilityCheck(unittest.TestCase):
    def test_missing_required_path_blocks_before_session(self):
        tmp = tempfile.mkdtemp(prefix="capability_")
        for missing in ("/session", "/permission", "/question"):
            with self.subTest(missing=missing):
                ctx = _exec_ctx(tmp)
                client = FakeClient(
                    "http://127.0.0.1:4096",
                    ctx["cloud_root"],
                    doc_paths={
                        path: {}
                        for path in ("/session", "/permission", "/question")
                        if path != missing
                    },
                )
                manager = FakeManager(base_commit=ctx["base_commit"])
                server = FakeServerStarter()
                executor, _factory_calls = _make_executor(
                    manager, client, ScriptedRunner(), server
                )
                executor.preflight(ctx)
                with self.assertRaises(PlanExecutionError) as raised:
                    executor.execute(ctx)
                self.assertTrue(raised.exception.stage)
                self.assertEqual(client.created_sessions, [])
                self.assertEqual(server.stops, 1)


class TestOwnedPermissions(unittest.TestCase):
    def test_owned_parent_and_child_replied_once_and_foreign_ignored(self):
        tmp = tempfile.mkdtemp(prefix="permissions_")
        ctx = _exec_ctx(tmp)
        _write_record(ctx, commit=ctx["base_commit"])
        client = FakeClient(
            "http://127.0.0.1:4096",
            ctx["cloud_root"],
            events_batches=[
                [
                    _skill_event(),
                    _permission_event("per_parent", "ses_parent"),
                    _permission_event("per_child", "ses_child"),
                    _permission_event("per_parent", "ses_parent"),
                    _permission_event("per_foreign", "ses_foreign"),
                ]
            ],
            permissions=[
                _permission("per_parent", "ses_parent"),
                _permission("per_child", "ses_child"),
                _permission("per_foreign", "ses_foreign"),
            ],
        )
        manager = FakeManager(base_commit=ctx["base_commit"])
        server = FakeServerStarter()
        runner = ScriptedRunner(
            base_commit=ctx["base_commit"], final_sha=ctx["base_commit"], changed=False
        )
        executor, _factory_calls = _make_executor(manager, client, runner, server)
        executor.preflight(ctx)

        result = executor.execute(ctx)

        self.assertEqual(result["status"], "no_changes")
        replied = {entry["id"]: entry["reply"] for entry in client.permission_replies}
        self.assertEqual(replied, {"per_parent": "once", "per_child": "once"})
        self.assertNotIn("per_foreign", replied)
        self.assertNotIn("always", [entry["reply"] for entry in client.permission_replies])
        self.assertEqual(len(client.permission_replies), 2)
        self.assertEqual(server.stops, 1)


class TestRecommendationQuestions(unittest.TestCase):
    def test_single_marked_option_answered_once_and_recorded(self):
        tmp = tempfile.mkdtemp(prefix="recommend_")
        ctx = _exec_ctx(tmp)
        _write_record(ctx, commit=ctx["base_commit"])
        label = "Use the frozen scope (recommended)"
        question = _question(
            "que_1",
            "ses_parent",
            "Which scope and acceptance rule should the run use?",
            [
                {
                    "label": label,
                    "description": "covers scope and acceptance criteria",
                },
                {"label": "Ask a human"},
            ],
        )
        client = FakeClient(
            "http://127.0.0.1:4096",
            ctx["cloud_root"],
            events_batches=[
                [
                    _skill_event(),
                    _question_event("que_1", "ses_parent"),
                    _question_event("que_1", "ses_parent"),
                ]
            ],
            questions=[question],
        )
        manager = FakeManager(base_commit=ctx["base_commit"])
        server = FakeServerStarter()
        runner = ScriptedRunner(
            base_commit=ctx["base_commit"], final_sha=ctx["base_commit"], changed=False
        )
        executor, _factory_calls = _make_executor(manager, client, runner, server)
        executor.preflight(ctx)

        result = executor.execute(ctx)

        self.assertEqual(result["status"], "no_changes")
        self.assertEqual(len(client.question_replies), 1)
        self.assertEqual(client.question_replies[0]["id"], "que_1")
        self.assertEqual(client.question_replies[0]["answers"], [[label]])
        self.assertEqual(len(manager.decisions), 1)
        decision = manager.decisions[0]
        self.assertIn(label, json.dumps(decision["answer"]))
        basis = json.dumps(decision["basis"]).lower()
        self.assertTrue("marker" in basis or "recommend" in basis)
        self.assertIn("Which scope", json.dumps(decision["question"]))
        self.assertEqual(server.stops, 1)


class TestManualQuestionTakeover(unittest.TestCase):
    def test_unresolved_question_waits_then_manual_reply_resumes(self):
        tmp = tempfile.mkdtemp(prefix="takeover_")
        ctx = _exec_ctx(tmp)
        _write_record(ctx, commit=ctx["base_commit"])
        question = _question(
            "que_2",
            "ses_parent",
            "Which approach should be taken?",
            [{"label": "Option A"}, {"label": "Option B"}],
        )
        client = FakeClient(
            "http://127.0.0.1:4096",
            ctx["cloud_root"],
            events_batches=[
                [_skill_event(), _question_event("que_2", "ses_parent")]
            ],
            questions=[question],
        )
        manager = FakeManager(base_commit=ctx["base_commit"])
        server = FakeServerStarter()
        runner = ScriptedRunner(
            base_commit=ctx["base_commit"], final_sha=ctx["base_commit"], changed=False
        )
        executor, factory_calls = _make_executor(manager, client, runner, server)
        executor.preflight(ctx)

        waiting = executor.execute(ctx)

        self.assertEqual(waiting["status"], "waiting_input")
        self.assertEqual(waiting["stage"], "waiting_input")
        record = manager.inspect("job-1")
        self.assertIn("Which approach", json.dumps(record["question"]))
        self.assertEqual(record["session_id"], "ses_parent")
        self.assertTrue(record["session_url"].startswith(server.url))
        self.assertEqual(client.question_replies, [])
        self.assertEqual(server.stops, 0)

        client.pending_questions = []
        client.events_batches.append(
            [
                _skill_event(),
                {"type": "question.replied", "properties": {"requestID": "que_2"}},
                _idle_event(),
            ]
        )

        resumed = executor.execute(ctx)

        self.assertEqual(resumed["status"], "no_changes")
        self.assertEqual(client.question_replies, [])
        self.assertEqual(client.session_status()["ses_parent"]["type"], "idle")
        self.assertEqual(server.stops, 1)
        self.assertEqual(len(factory_calls), 1)


class TestCompletionEvidenceGate(unittest.TestCase):
    def test_idle_without_completion_evidence_never_publishes(self):
        tmp = tempfile.mkdtemp(prefix="idle_")
        ctx = _exec_ctx(tmp)
        _write_record(ctx, commit=ctx["final_sha"])
        client = FakeClient(
            "http://127.0.0.1:4096",
            ctx["cloud_root"],
            events_batches=[[_idle_event()]],
        )
        manager = FakeManager(base_commit=ctx["base_commit"])
        server = FakeServerStarter()
        runner = ScriptedRunner(
            base_commit=ctx["base_commit"], final_sha=ctx["final_sha"], changed=True
        )
        executor, _factory_calls = _make_executor(manager, client, runner, server)
        executor.preflight(ctx)

        result = executor.execute(ctx)

        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["stage"], "completion")
        self.assertEqual(_git_calls(runner, "push"), [])
        self.assertEqual(_gh_calls(runner, "create"), [])
        self.assertEqual(server.stops, 1)


class TestPublication(unittest.TestCase):
    def _setup(self, tmp, *, runner, record_commit):
        ctx = _exec_ctx(tmp)
        _write_record(ctx, commit=record_commit)
        client = FakeClient(
            "http://127.0.0.1:4096",
            ctx["cloud_root"],
            events_batches=[[_skill_event(), _idle_event()]],
        )
        manager = FakeManager(base_commit=ctx["base_commit"])
        server = FakeServerStarter()
        executor, _factory_calls = _make_executor(manager, client, runner, server)
        executor.preflight(ctx)
        return ctx, manager, client, server, executor

    def test_valid_changed_result_pushes_once_and_creates_one_pr(self):
        tmp = tempfile.mkdtemp(prefix="publish_")
        runner = ScriptedRunner(
            base_commit=BASE_SHA,
            final_sha=FINAL_SHA,
            changed=True,
            pr_list=[[]],
            create_url=PR_URL,
        )
        ctx, manager, _client, server, executor = self._setup(
            tmp, runner=runner, record_commit=FINAL_SHA
        )

        result = executor.execute(ctx)

        self.assertEqual(result["status"], "completed")
        self.assertEqual(result["final_commit"], FINAL_SHA)
        self.assertEqual(result["pr_url"], PR_URL)
        pushes = _git_calls(runner, "push")
        self.assertEqual(len(pushes), 1)
        self.assertEqual(
            pushes[0],
            ["git", "-C", ctx["cloud_root"], "push", "origin", ctx["delivery_branch"]],
        )
        self.assertNotIn("--force", pushes[0])
        creates = _gh_calls(runner, "pr", "create")
        self.assertEqual(len(creates), 1)
        create = creates[0]
        for flag, value in (
            ("--repo", ctx["canonical_repo"]),
            ("--base", ctx["base_branch"]),
            ("--head", ctx["delivery_branch"]),
        ):
            self.assertIn(flag, create)
            self.assertEqual(create[create.index(flag) + 1], value)
        self.assertNotIn("Closes", " ".join(create))
        self.assertEqual(_gh_calls(runner, "pr", "merge"), [])
        self.assertEqual(manager.inspect("job-1")["final_commit"], FINAL_SHA)
        self.assertEqual(manager.inspect("job-1")["pr_url"], PR_URL)
        self.assertEqual(server.stops, 1)

    def test_matching_existing_pr_is_reused_without_create(self):
        tmp = tempfile.mkdtemp(prefix="existing_pr_")
        existing = {
            "url": PR_URL,
            "state": "OPEN",
            "headRefName": "ai-workflow/job-1",
            "baseRefName": "main",
        }
        runner = ScriptedRunner(
            base_commit=BASE_SHA,
            final_sha=FINAL_SHA,
            changed=True,
            pr_list=[[existing]],
            create_url=None,
        )
        ctx, manager, _client, server, executor = self._setup(
            tmp, runner=runner, record_commit=FINAL_SHA
        )

        result = executor.execute(ctx)

        self.assertEqual(result["status"], "completed")
        self.assertEqual(result["pr_url"], PR_URL)
        self.assertEqual(_gh_calls(runner, "pr", "create"), [])
        self.assertGreaterEqual(len(_gh_calls(runner, "pr", "list")), 1)
        self.assertEqual(len(_git_calls(runner, "push")), 1)
        self.assertEqual(server.stops, 1)

    def test_lost_create_response_is_reconciled_after_relist(self):
        tmp = tempfile.mkdtemp(prefix="lost_create_")
        existing = {"url": PR_URL, "state": "OPEN"}
        runner = ScriptedRunner(
            base_commit=BASE_SHA,
            final_sha=FINAL_SHA,
            changed=True,
            pr_list=[[]],
            pr_list_after_create=[existing],
            create_rc=1,
            create_url=None,
        )
        ctx, manager, _client, server, executor = self._setup(
            tmp, runner=runner, record_commit=FINAL_SHA
        )

        result = executor.execute(ctx)

        self.assertEqual(result["status"], "completed")
        self.assertEqual(result["pr_url"], PR_URL)
        self.assertEqual(len(_gh_calls(runner, "pr", "create")), 1)
        self.assertGreaterEqual(len(_gh_calls(runner, "pr", "list")), 1)
        self.assertEqual(manager.inspect("job-1")["pr_url"], PR_URL)
        self.assertEqual(server.stops, 1)

    def test_publication_failure_reports_stage_and_keeps_commit(self):
        cases = [
            ("push", {"push_rc": 1, "create_rc": 0, "pr_list": [[], []]}),
            ("create", {"push_rc": 0, "create_rc": 1, "pr_list": [[], []]}),
        ]
        for label, options in cases:
            with self.subTest(failure=label):
                tmp = tempfile.mkdtemp(prefix="publish_fail_")
                runner = ScriptedRunner(
                    base_commit=BASE_SHA,
                    final_sha=FINAL_SHA,
                    changed=True,
                    **options
                )
                ctx, manager, _client, server, executor = self._setup(
                    tmp, runner=runner, record_commit=FINAL_SHA
                )

                result = executor.execute(ctx)

                self.assertEqual(result["status"], "failed")
                self.assertEqual(result["stage"], "publishing")
                self.assertNotEqual(result["status"], "completed")
                self.assertEqual(manager.inspect("job-1")["final_commit"], FINAL_SHA)
                self.assertIsNone(manager.inspect("job-1")["pr_url"])
                self.assertEqual(server.stops, 1)


class TestNoChanges(unittest.TestCase):
    def test_unchanged_result_skips_push_and_pr(self):
        tmp = tempfile.mkdtemp(prefix="no_changes_")
        ctx = _exec_ctx(tmp)
        _write_record(ctx, commit=ctx["base_commit"])
        client = FakeClient(
            "http://127.0.0.1:4096",
            ctx["cloud_root"],
            events_batches=[[_skill_event(), _idle_event()]],
        )
        manager = FakeManager(base_commit=ctx["base_commit"])
        server = FakeServerStarter()
        runner = ScriptedRunner(
            base_commit=ctx["base_commit"], final_sha=ctx["base_commit"], changed=False
        )
        executor, _factory_calls = _make_executor(manager, client, runner, server)
        executor.preflight(ctx)

        result = executor.execute(ctx)

        self.assertEqual(result["status"], "no_changes")
        self.assertEqual(result["final_commit"], ctx["base_commit"])
        self.assertEqual(_git_calls(runner, "push"), [])
        self.assertEqual(_gh_calls(runner, "create"), [])
        self.assertEqual(server.stops, 1)


class TestCancellation(unittest.TestCase):
    def test_cancelled_before_publication_stops_owned_server_without_push(self):
        tmp = tempfile.mkdtemp(prefix="cancel_")
        ctx = _exec_ctx(tmp)
        _write_record(ctx, commit=ctx["final_sha"])
        client = FakeClient(
            "http://127.0.0.1:4096",
            ctx["cloud_root"],
            events_batches=[[_skill_event(), _idle_event()]],
        )
        manager = FakeManager(base_commit=ctx["base_commit"])
        server = FakeServerStarter()

        def cancel_after_stream(_client):
            manager.update("job-1", status="cancelled", stage="cancelled")

        client.after_stream = cancel_after_stream
        runner = ScriptedRunner(
            base_commit=ctx["base_commit"], final_sha=ctx["final_sha"], changed=True
        )
        executor, factory_calls = _make_executor(manager, client, runner, server)
        executor.preflight(ctx)

        first = executor.execute(ctx)

        self.assertEqual(first["status"], "cancelled")
        self.assertEqual(server.stops, 1)
        self.assertEqual(_git_calls(runner, "push"), [])
        self.assertEqual(_gh_calls(runner, "create"), [])
        self.assertEqual(manager.inspect("job-1")["status"], "cancelled")

        stream_calls = client.stream_calls
        second = executor.execute(ctx)

        self.assertEqual(second["status"], "cancelled")
        self.assertEqual(manager.inspect("job-1")["status"], "cancelled")
        self.assertEqual(len(server.contexts), 1)
        self.assertEqual(server.stops, 1)
        self.assertEqual(client.stream_calls, stream_calls)
        self.assertEqual(len(factory_calls), 1)
        self.assertEqual(_git_calls(runner, "push"), [])
        self.assertEqual(_gh_calls(runner, "create"), [])


if __name__ == "__main__":
    unittest.main()
