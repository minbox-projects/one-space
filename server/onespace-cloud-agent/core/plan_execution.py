#!/usr/bin/env python3
"""Drive one accepted plan job through coding, intervention and publication.

The cloud controller owns one loopback OpenCode server per job, scopes every
request to the prepared clone, and translates only the exact recommended
question shape into an answer.  Completion comes from the coding-owned
workflow record and the delivery branch HEAD, never from model prose.
"""

import json
import os
import re
import signal
import socket
import subprocess
import threading
import time
import urllib.parse
import urllib.request

from core.plan_opencode import OpenCodeClient, owned_session_ids, recommended_answers

TERMINAL_STATUSES = frozenset({"completed", "no_changes", "failed", "cancelled"})
_SHA_PATTERN = re.compile(r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$")
_REQUIRED_CAPABILITIES = ("/session", "/permission", "/question")
_REQUIRED_NATIVE_ROLES = (
    "backend",
    "frontend",
    "test",
    "git-operator",
    "file-explorer",
    "documentation-maintainer",
    "spec-review",
    "standards-review",
    "researcher",
)


class PlanExecutionError(Exception):
    """A plan execution failure carrying its observable stage."""

    def __init__(self, stage, message):
        super().__init__(message)
        self.stage = stage
        self.message = message


# ----------------------------------------------------------------------
# Context and root mapping
# ----------------------------------------------------------------------
def _request_envelope(manager, job_id):
    try:
        request = manager.get_request(job_id)
    except Exception:  # noqa: BLE001 - context building remains best effort
        return {}
    if not isinstance(request, dict):
        return {}
    envelope = request.get("envelope")
    return envelope if isinstance(envelope, dict) else request


def _read_schedule(plan_dir):
    path = os.path.join(plan_dir, "tasks", "execution-order.yaml")
    try:
        with open(path, "r", encoding="utf-8") as handle:
            return handle.read().strip()
    except OSError:
        return ""


def build_execution_context(manager, task):
    """Build the trusted execution context from a durable task projection."""
    task = task if isinstance(task, dict) else {}
    job_id = task.get("id") or task.get("job_id")
    envelope = _request_envelope(manager, job_id)
    repository = task.get("repository")
    if not isinstance(repository, dict):
        repository = envelope.get("repository")
    if not isinstance(repository, dict):
        repository = {}
    plan = envelope.get("plan") if isinstance(envelope.get("plan"), dict) else {}
    workspace = task.get("workspace")
    if not isinstance(workspace, dict):
        workspace = envelope.get("workspace")
    if not isinstance(workspace, dict):
        workspace = {}

    plan_id = task.get("plan_id") or plan.get("plan_id")
    cloud_root = task.get("remote_root") or task.get("cloud_root")
    if not cloud_root:
        cloud_root = task.get("workspace_root")
    plan_dir = task.get("plan_dir")
    if not plan_dir and cloud_root and plan_id:
        plan_dir = os.path.join(cloud_root, ".ai-workflow", "plans", plan_id)
    local_root = task.get("local_root") or workspace.get("root_path")
    delivery_branch = task.get("branch") or (
        "ai-workflow/%s" % job_id if job_id else None
    )
    base_branch = task.get("base_branch") or repository.get("base_branch")
    remote_url = task.get("remote_url") or repository.get("remote_url")
    canonical_repo = (
        task.get("canonical_repo") or task.get("repo") or repository.get("canonical")
    )
    schedule_text = task.get("schedule_text")
    if schedule_text is None:
        schedule_text = _read_schedule(plan_dir) if plan_dir else ""
    policy = task.get("policy") or envelope.get("policy") or {}

    return {
        "job_id": job_id,
        "plan_id": plan_id,
        "plan_dir": plan_dir,
        "cloud_root": cloud_root,
        "local_root": local_root,
        "delivery_branch": delivery_branch,
        "base_branch": base_branch,
        "base_commit": task.get("base_commit"),
        "remote_url": remote_url,
        "canonical_repo": canonical_repo,
        "schedule_text": schedule_text or "",
        "policy": policy if isinstance(policy, dict) else {},
        "title": task.get("plan_title") or task.get("title") or plan.get("title"),
    }


def _local_prefix(context):
    local = context.get("local_root")
    if not isinstance(local, str) or not local:
        return None
    return local.rstrip("/") or "/"


def map_project_path(context, path):
    """Map one original-root-prefixed path to its cloud equivalent."""
    if not isinstance(path, str):
        return path
    local = _local_prefix(context)
    cloud = context.get("cloud_root")
    if not local or not isinstance(cloud, str) or not cloud:
        return path
    if path == local:
        return cloud
    prefix = local if local.endswith("/") else local + "/"
    if path.startswith(prefix):
        return cloud.rstrip("/") + "/" + path[len(prefix):]
    return path


def map_command_paths(context, command):
    """Rewrite every original-root occurrence in a command string."""
    if not isinstance(command, str):
        return command
    local = _local_prefix(context)
    cloud = context.get("cloud_root")
    if not local or not isinstance(cloud, str) or not cloud:
        return command
    mapped = command.replace(local, cloud.rstrip("/"))
    alternate = local.rstrip("/")
    if alternate != local:
        mapped = mapped.replace(alternate, cloud.rstrip("/"))
    return mapped


def unmapped_paths(context, paths):
    """Return absolute paths that resolve outside both declared roots."""
    local = _local_prefix(context)
    cloud = context.get("cloud_root")
    cloud = cloud.rstrip("/") if isinstance(cloud, str) and cloud else None
    missing = []
    for path in paths or []:
        if not isinstance(path, str) or not path:
            missing.append(path)
            continue
        if not os.path.isabs(path):
            continue
        if local and (path == local or path.startswith(local.rstrip("/") + "/")):
            continue
        if cloud and (path == cloud or path.startswith(cloud + "/")):
            continue
        missing.append(path)
    return missing


def build_execution_prompt(context):
    """Render the trusted execution prompt with mapped real paths."""
    plan_dir = map_project_path(context, context.get("plan_dir") or "")
    cloud_root = context.get("cloud_root") or ""
    local_root = context.get("local_root") or ""
    schedule = map_command_paths(context, context.get("schedule_text") or "")
    policy = context.get("policy") if isinstance(context.get("policy"), dict) else {}
    lines = [
        "Execute the frozen ai-workflow plan in the prepared cloud clone.",
        "",
        "- Absolute cloud plan directory: %s" % plan_dir,
        "- Cloud project root: %s" % cloud_root,
        "- Original local project root mapping: %s -> %s" % (local_root, cloud_root),
        "- Delivery branch: %s" % (context.get("delivery_branch") or ""),
        "- Base branch: %s" % (context.get("base_branch") or ""),
        "- Base commit: %s" % (context.get("base_commit") or ""),
        "",
        "Frozen schedule (authoritative, do not rewrite):",
        schedule or "(no split task schedule)",
        "",
        "Use the installed `coding` skill through the skill tool with name",
        "`coding` before editing. Follow the frozen scope, tasks, tests and the",
        "workflow gates exactly; never waive reviews, tests or integration, and",
        "never rewrite the frozen plan or its digests. Apply the root mapping",
        "above to real scope and command paths only.",
        "",
        "The prepared delivery branch is the only publication unit. Do not push",
        "the base branch and do not merge any remote pull request automatically.",
        "Tool permissions are single-use: request and accept `once`, never",
        "assume `always`. Only a question with exactly one marked recommended",
        "option may be answered automatically; every other question waits for",
        "a human. Write the completed implementation record with the matching",
        "full final commit into the cloud plan directory when done.",
    ]
    if policy.get("auto_merge") is False:
        lines.append("Automatic remote merge is disabled for this job.")
    return "\n".join(lines)


# ----------------------------------------------------------------------
# Completion evidence
# ----------------------------------------------------------------------
def _parse_yaml_mapping(text):
    """Parse a deliberately small YAML mapping without third-party code."""
    if not isinstance(text, str):
        raise ValueError("record must be text")
    values = {}
    for raw_line in text.splitlines():
        stripped = raw_line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        indentation = len(raw_line) - len(raw_line.lstrip(" "))
        if indentation:
            # Nested sequence/block details are not needed for evidence; the
            # top-level status and commit remain authoritative.
            continue
        if ":" not in stripped:
            raise ValueError("malformed YAML line: %s" % stripped)
        key, value = stripped.split(":", 1)
        key = key.strip()
        if not re.fullmatch(r"[A-Za-z0-9_.-]+", key or ""):
            raise ValueError("malformed YAML key: %s" % key)
        value = value.strip()
        if value.startswith("[") and not value.endswith("]"):
            raise ValueError("unterminated YAML sequence: %s" % value)
        if value.startswith("{") and not value.endswith("}"):
            raise ValueError("unterminated YAML mapping: %s" % value)
        if len(value) >= 2 and value[0] == value[-1] and value[0] in ("'", '"'):
            value = value[1:-1]
        values[key] = value
    if not values:
        raise ValueError("empty YAML mapping")
    return values


def _run_git(git_runner, cmd):
    try:
        return git_runner(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
    except FileNotFoundError:
        return None
    except OSError:
        return None


def _dirty_paths(stdout):
    """Return source-dirty porcelain entries, excluding workflow plan state."""
    dirty = []
    for line in stdout.splitlines():
        if not line.strip():
            continue
        path = line[3:] if len(line) > 3 else ""
        if " -> " in path:
            parts = [part.strip().strip('"') for part in path.split(" -> ", 1)]
        else:
            parts = [path.strip().strip('"')]
        if all(
            part == ".ai-workflow" or part.startswith(".ai-workflow/")
            for part in parts
        ):
            continue
        dirty.append(line)
    return dirty


def completion_evidence(context, *, observed_skill, record_path, git_runner, clock=time.time):
    """Validate the coding-owned completed record against the delivery HEAD."""
    result = {
        "ok": False,
        "reason": None,
        "final_commit": None,
        "changed": False,
        "checked_at": clock(),
    }
    if not observed_skill:
        result["reason"] = "the coding skill was not observed"
        return result
    try:
        with open(record_path, "r", encoding="utf-8") as handle:
            record = _parse_yaml_mapping(handle.read())
    except (OSError, ValueError) as exc:
        result["reason"] = "invalid implementation record: %s" % exc
        return result

    if record.get("status") != "completed":
        result["reason"] = "implementation record is not completed"
        return result
    commit = record.get("commit")
    if not isinstance(commit, str) or not _SHA_PATTERN.match(commit):
        result["reason"] = "implementation record lacks a full commit"
        return result

    root = context.get("cloud_root")
    if not isinstance(root, str) or not root:
        result["reason"] = "missing cloud root"
        return result
    head_proc = _run_git(git_runner, ["git", "-C", root, "rev-parse", "HEAD"])
    head = (getattr(head_proc, "stdout", "") or "").strip() if head_proc else ""
    if head_proc is None or getattr(head_proc, "returncode", 1) != 0 or head != commit:
        result["reason"] = "recorded commit does not match delivery HEAD"
        return result

    status_proc = _run_git(git_runner, ["git", "-C", root, "status", "--porcelain"])
    if status_proc is None or getattr(status_proc, "returncode", 1) != 0:
        result["reason"] = "could not inspect the delivery worktree"
        return result
    if _dirty_paths(getattr(status_proc, "stdout", "") or ""):
        result["reason"] = "delivery worktree is not clean"
        return result

    base = context.get("base_commit")
    if not isinstance(base, str) or not base:
        result["reason"] = "missing captured base commit"
        return result
    diff_proc = _run_git(
        git_runner, ["git", "-C", root, "diff", "--quiet", base, "HEAD"]
    )
    if diff_proc is None:
        result["reason"] = "could not compare the delivery tree"
        return result
    changed = getattr(diff_proc, "returncode", 0) != 0

    result.update({"ok": True, "reason": None, "final_commit": commit, "changed": changed})
    return result


# ----------------------------------------------------------------------
# Owned loopback server
# ----------------------------------------------------------------------
def _free_port():
    handle = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        handle.bind(("127.0.0.1", 0))
        return handle.getsockname()[1]
    finally:
        handle.close()


def _default_server_starter(context):
    cloud_root = context.get("cloud_root")
    if not isinstance(cloud_root, str) or not os.path.isdir(cloud_root):
        raise PlanExecutionError("opencode", "missing cloud root for OpenCode server")
    port = _free_port()
    command = [
        "opencode",
        "serve",
        "--hostname",
        "127.0.0.1",
        "--port",
        str(port),
    ]
    try:
        process = subprocess.Popen(
            command,
            cwd=cloud_root,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True,
        )
    except OSError as exc:
        raise PlanExecutionError("opencode", "could not start OpenCode: %s" % exc)

    stopped = {"value": False}

    def stop():
        if stopped["value"]:
            return
        stopped["value"] = True
        try:
            os.killpg(os.getpgid(process.pid), signal.SIGTERM)
        except (ProcessLookupError, PermissionError):
            return
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(os.getpgid(process.pid), signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                pass

    url = "http://127.0.0.1:%d" % port
    query = urllib.parse.urlencode({"directory": cloud_root})
    deadline = time.time() + 30.0
    while time.time() < deadline:
        if process.poll() is not None:
            stop()
            raise PlanExecutionError("opencode", "OpenCode server exited during startup")
        try:
            with urllib.request.urlopen("%s/doc?%s" % (url, query), timeout=1.0) as resp:
                resp.read()
            return {"url": url, "process": process, "stop": stop}
        except OSError:
            time.sleep(0.1)
    stop()
    raise PlanExecutionError("opencode", "OpenCode server did not become ready")


# ----------------------------------------------------------------------
# Executor
# ----------------------------------------------------------------------
class PlanExecutor:
    """Resumable controller for one accepted plan job."""

    def __init__(
        self,
        manager,
        *,
        client_factory=None,
        runner=None,
        opener=None,
        server_starter=None,
        clock=time.time,
        sleep=time.sleep,
        max_wait_seconds=3600,
    ):
        self.manager = manager
        self._runner = runner or subprocess.run
        self._opener = opener
        self._server_starter = server_starter or _default_server_starter
        self._clock = clock or time.time
        self._sleep = sleep or time.sleep
        self._max_wait_seconds = max_wait_seconds
        self._client_factory = client_factory or (
            lambda url, cloud_root: OpenCodeClient(
                url, cloud_root, opener=self._opener, timeout=30.0
            )
        )
        self._states = {}

    # ------------------------------------------------------------------
    # Preflight
    # ------------------------------------------------------------------
    def _command(self, stage, command):
        try:
            return self._runner(
                command,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False,
            )
        except (FileNotFoundError, OSError) as exc:
            raise PlanExecutionError(stage, "%s is unavailable: %s" % (command[0], exc))

    def preflight(self, context):
        cloud_root = context.get("cloud_root")
        if not isinstance(cloud_root, str) or not cloud_root:
            raise PlanExecutionError("contract", "missing cloud project root")
        for relative in (".ai-workflow/AGENTS.md", "AGENTS.md"):
            if not os.path.isfile(os.path.join(cloud_root, relative)):
                raise PlanExecutionError(
                    "contract", "missing required project contract: %s" % relative
                )

        proc = self._command("workflow", ["ai-workflow", "--version"])
        if getattr(proc, "returncode", 1) != 0:
            raise PlanExecutionError("workflow", "ai-workflow CLI is unavailable")

        proc = self._command("opencode", ["opencode", "--version"])
        if getattr(proc, "returncode", 1) != 0:
            raise PlanExecutionError("opencode", "opencode CLI is unavailable")

        proc = self._command("skill", ["opencode", "debug", "skill"])
        output = "%s\n%s" % (
            getattr(proc, "stdout", "") or "",
            getattr(proc, "stderr", "") or "",
        )
        if getattr(proc, "returncode", 1) != 0 or "coding" not in output.lower():
            raise PlanExecutionError("skill", "the coding skill is not installed")

        proc = self._command("github", ["gh", "auth", "status"])
        if getattr(proc, "returncode", 1) != 0:
            raise PlanExecutionError("github", "GitHub authentication is unavailable")

        proc = self._command("roles", ["opencode", "debug", "config"])
        if getattr(proc, "returncode", 1) != 0:
            raise PlanExecutionError(
                "roles", "could not read the native role configuration"
            )
        try:
            configuration = json.loads(getattr(proc, "stdout", "") or "")
        except (TypeError, ValueError):
            raise PlanExecutionError(
                "roles", "the native role configuration is not valid JSON"
            )
        agents = configuration.get("agent") if isinstance(configuration, dict) else None
        if not isinstance(agents, dict):
            raise PlanExecutionError(
                "roles", "the native role configuration has no agent mapping"
            )
        missing_roles = [role for role in _REQUIRED_NATIVE_ROLES if role not in agents]
        if missing_roles:
            raise PlanExecutionError(
                "roles", "missing native roles: %s" % ", ".join(missing_roles)
            )

        plan_dir = context.get("plan_dir")
        if not isinstance(plan_dir, str) or not os.path.isabs(plan_dir):
            raise PlanExecutionError("plan", "missing absolute cloud plan directory")
        proc = self._command(
            "plan", ["ai-workflow", "plan", "validate", "--plan", plan_dir]
        )
        if getattr(proc, "returncode", 1) != 0:
            raise PlanExecutionError("plan", "the frozen plan failed validation")
        return True

    # ------------------------------------------------------------------
    # Entry point
    # ------------------------------------------------------------------
    def execute(self, context):
        job_id = context.get("job_id")
        record = self.manager.inspect(job_id)
        if record is None:
            raise PlanExecutionError("not_found", "unknown plan job")
        if record.get("status") in TERMINAL_STATUSES:
            return record

        state = self._states.get(job_id)
        if state is None:
            state = self._prepare_state(context)
            self._states[job_id] = state

        return self._run_until_outcome(state, context)

    def _prepare_state(self, context):
        job_id = context["job_id"]
        state = {
            "job_id": job_id,
            "server": None,
            "client": None,
            "session_id": None,
            "prompt_sent": False,
            "permissions_replied": set(),
            "questions_replied": set(),
            "manual_replies": set(),
            "events": [],
            "event_lock": threading.Lock(),
            "idle_sessions": set(),
            "observed_skill": False,
            "stream_thread": None,
            "stream_stop": None,
            "stream_error": None,
            "waiting": False,
            "stopped": False,
        }
        try:
            self.preflight(context)
        except PlanExecutionError as exc:
            self._record_failure(job_id, exc.stage, exc.message)
            raise

        try:
            server = self._server_starter(context)
        except PlanExecutionError as exc:
            self._record_failure(job_id, exc.stage, exc.message)
            raise
        except Exception as exc:  # noqa: BLE001 - server startup failure is projected
            self._record_failure(job_id, "opencode", str(exc))
            raise PlanExecutionError("opencode", str(exc))
        state["server"] = server
        process = server.get("process") if isinstance(server, dict) else None
        server_pid = getattr(process, "pid", None)
        if isinstance(server_pid, int) and not isinstance(server_pid, bool) and server_pid > 0:
            self.manager.update(job_id, server_pid=server_pid)

        try:
            client = self._client_factory(server["url"], context["cloud_root"])
            state["client"] = client
            document = client.doc()
            paths = document.get("paths") if isinstance(document, dict) else None
            if not isinstance(paths, dict):
                paths = {}
            missing = [path for path in _REQUIRED_CAPABILITIES if path not in paths]
            if missing:
                raise PlanExecutionError(
                    "capabilities",
                    "OpenCode lacks required v1 endpoints: %s" % ", ".join(missing),
                )
            session = client.create_session(
                title="Plan %s" % (context.get("plan_id") or job_id),
                parent_id=None,
            )
            session_id = session.get("id") if isinstance(session, dict) else None
            if not isinstance(session_id, str) or not session_id:
                raise PlanExecutionError(
                    "capabilities", "OpenCode did not return a parent session id"
                )
            state["session_id"] = session_id
            self.manager.update(
                job_id,
                status="running",
                stage="coding",
                session_id=session_id,
                session_url=server["url"],
                question=None,
            )
            client.prompt_async(session_id, build_execution_prompt(context))
            state["prompt_sent"] = True
        except PlanExecutionError as exc:
            self._stop_server(state)
            self._record_failure(job_id, exc.stage, exc.message)
            raise
        except Exception as exc:  # noqa: BLE001 - execution setup failure is projected
            self._stop_server(state)
            self._record_failure(job_id, "capabilities", str(exc))
            raise PlanExecutionError("capabilities", str(exc))
        return state

    # ------------------------------------------------------------------
    # Event loop
    # ------------------------------------------------------------------
    def _run_until_outcome(self, state, context):
        job_id = context["job_id"]
        self._ensure_stream(state)
        owned = self._owned_sessions(state)
        deadline = self._clock() + self._max_wait_seconds

        while True:
            record = self.manager.inspect(job_id) or {}
            if record.get("status") == "cancelled":
                self._stop_server(state)
                return self.manager.inspect(job_id)

            self._drain_events(state, owned)
            self._reply_pending_permissions(state, owned)
            unresolved = self._reply_pending_questions(state, owned)
            if unresolved:
                self._set_waiting(state, context, unresolved)
                return self.manager.inspect(job_id)

            if state.get("waiting"):
                state["waiting"] = False
                self.manager.update(
                    job_id,
                    status="running",
                    stage="coding",
                    question=None,
                    updated_at=self._clock(),
                )

            if self._is_idle(state, owned) and state.get("prompt_sent"):
                return self._complete(state, context)

            thread = state.get("stream_thread")
            if thread is None or not thread.is_alive():
                return self._fail(
                    state,
                    context,
                    "execution",
                    "the event stream ended before completion",
                )

            if self._clock() >= deadline:
                return self._fail(
                    state, context, "timeout", "plan execution exceeded its time limit"
                )
            self._sleep(0.05)

    def _ensure_stream(self, state):
        thread = state.get("stream_thread")
        if thread is not None and thread.is_alive():
            return thread
        with state["event_lock"]:
            state["events"] = []
            state["idle_sessions"] = set()
        stop = threading.Event()
        state["stream_stop"] = stop
        client = state["client"]

        def collect(event):
            self._record_event(state, event)

        def run():
            try:
                client.stream_events(stop, collect)
            except Exception as exc:  # noqa: BLE001 - surfaced by the controller
                state["stream_error"] = exc

        thread = threading.Thread(target=run, daemon=True)
        state["stream_thread"] = thread
        thread.start()
        thread.join(timeout=0.05)
        return thread

    def _record_event(self, state, event):
        if not isinstance(event, dict):
            return
        etype = event.get("type")
        properties = event.get("properties")
        if not isinstance(properties, dict):
            properties = {}
        with state["event_lock"]:
            state["events"].append(event)
            if etype == "session.idle":
                session_id = properties.get("sessionID") or properties.get("sessionId")
                if session_id:
                    state["idle_sessions"].add(session_id)
            elif etype == "message.part.updated":
                part = properties.get("part")
                if not isinstance(part, dict):
                    part = {}
                state_obj = part.get("state")
                if not isinstance(state_obj, dict):
                    state_obj = {}
                input_obj = state_obj.get("input")
                if not isinstance(input_obj, dict):
                    input_obj = {}
                if (
                    part.get("type") == "tool"
                    and part.get("tool") == "skill"
                    and state_obj.get("status") == "completed"
                    and input_obj.get("name") == "coding"
                ):
                    state["observed_skill"] = True
            elif etype == "question.replied":
                request_id = properties.get("requestID") or properties.get("id")
                if request_id:
                    state["manual_replies"].add(request_id)

    def _drain_events(self, state, owned):
        with state["event_lock"]:
            events = state["events"]
            state["events"] = []
        for event in events:
            if event.get("type") != "permission.asked":
                continue
            properties = event.get("properties")
            if not isinstance(properties, dict):
                properties = {}
            request_id = properties.get("requestID") or properties.get("id")
            session_id = properties.get("sessionID")
            if request_id and session_id in owned:
                self._reply_permission(state, request_id)

    def _owned_sessions(self, state):
        parent = state.get("session_id")
        if not parent:
            return set()
        try:
            children = state["client"].children(parent)
        except Exception:  # noqa: BLE001 - ownership falls back to the parent
            return {parent}
        if not isinstance(children, list):
            children = []
        return owned_session_ids(parent, children)

    def _reply_permission(self, state, request_id):
        if not request_id or request_id in state["permissions_replied"]:
            return
        state["permissions_replied"].add(request_id)
        try:
            state["client"].reply_permission(request_id, reply="once")
        except Exception:  # noqa: BLE001 - polling will retry a lost reply
            state["permissions_replied"].discard(request_id)

    def _reply_pending_permissions(self, state, owned):
        try:
            pending = state["client"].permissions()
        except Exception:  # noqa: BLE001 - events still drive replies
            return
        if not isinstance(pending, list):
            return
        for item in pending:
            if not isinstance(item, dict):
                continue
            request_id = item.get("requestID") or item.get("id")
            session_id = item.get("sessionID")
            if request_id and session_id in owned:
                self._reply_permission(state, request_id)

    def _reply_pending_questions(self, state, owned):
        try:
            pending = state["client"].questions()
        except Exception:  # noqa: BLE001 - a transient poll failure retries later
            return []
        if not isinstance(pending, list):
            return []
        unresolved = []
        for request in pending:
            if not isinstance(request, dict):
                continue
            request_id = request.get("requestID") or request.get("id")
            session_id = request.get("sessionID")
            if session_id not in owned or not request_id:
                continue
            if request_id in state["questions_replied"]:
                continue
            if request_id in state["manual_replies"]:
                state["questions_replied"].add(request_id)
                continue
            answers = recommended_answers(request)
            if answers is None:
                unresolved.append(request)
                continue
            try:
                state["client"].reply_question(request_id, answers)
            except Exception:  # noqa: BLE001 - a failed reply stays pending
                unresolved.append(request)
                continue
            state["questions_replied"].add(request_id)
            self._record_decision(state, request, answers)
        return unresolved

    def _record_decision(self, state, request, answers):
        job_id = state.get("job_id")
        try:
            questions = [
                question.get("question")
                for question in request.get("questions") or []
                if isinstance(question, dict)
            ]
        except Exception:  # noqa: BLE001 - evidence must not block a reply
            questions = []
        summary = {
            "request_id": request.get("requestID") or request.get("id"),
            "questions": questions,
        }
        state["decision"] = {
            "question": summary,
            "answer": answers,
            "basis": "the single option label ends with a recommended marker",
        }
        try:
            self.manager.record_decision(
                job_id,
                summary,
                answers,
                "the single option label ends with a recommended marker",
            )
        except Exception:  # noqa: BLE001 - decision logging is best effort
            pass

    def _is_idle(self, state, owned):
        """Only the job parent session determines coding idle, never children."""
        parent = state.get("session_id")
        if not parent:
            return False
        if parent in state.get("idle_sessions", ()):
            return True
        try:
            statuses = state["client"].session_status()
        except Exception:  # noqa: BLE001 - events remain authoritative
            return False
        if not isinstance(statuses, dict):
            return False
        value = statuses.get(parent)
        return isinstance(value, dict) and value.get("type") == "idle"

    def _set_waiting(self, state, context, unresolved):
        state["waiting"] = True
        request = unresolved[0]
        questions = [
            question.get("question")
            for question in request.get("questions") or []
            if isinstance(question, dict)
        ]
        summary = {
            "request_id": request.get("requestID") or request.get("id"),
            "questions": questions,
            "session_id": state.get("session_id"),
        }
        self.manager.update(
            context["job_id"],
            status="waiting_input",
            stage="waiting_input",
            question=summary,
            session_id=state.get("session_id"),
            session_url=(state.get("server") or {}).get("url"),
            updated_at=self._clock(),
        )

    # ------------------------------------------------------------------
    # Completion and publication
    # ------------------------------------------------------------------
    def _complete(self, state, context):
        job_id = context["job_id"]
        record = self.manager.inspect(job_id) or {}
        if record.get("status") == "cancelled":
            self._stop_server(state)
            return self.manager.inspect(job_id)

        record_path = os.path.join(context["plan_dir"], "implementation.yaml")
        evidence = completion_evidence(
            context,
            observed_skill=state.get("observed_skill", False),
            record_path=record_path,
            git_runner=self._runner,
            clock=self._clock,
        )
        if not evidence.get("ok"):
            return self._fail(
                state,
                context,
                "completion",
                evidence.get("reason") or "completion evidence is invalid",
            )

        final_commit = evidence["final_commit"]
        now = self._clock()
        if not evidence.get("changed"):
            self.manager.update(
                job_id,
                status="no_changes",
                stage="no_changes",
                final_commit=final_commit,
                pr_url=None,
                finished_at=now,
                updated_at=now,
            )
            self._stop_server(state)
            return self.manager.inspect(job_id)

        self.manager.update(
            job_id,
            status="publishing",
            stage="publishing",
            final_commit=final_commit,
            updated_at=now,
        )
        record = self.manager.inspect(job_id) or {}
        if record.get("status") == "cancelled":
            self._stop_server(state)
            return self.manager.inspect(job_id)

        root = context["cloud_root"]
        branch = context["delivery_branch"]
        push = _run_git(
            self._runner, ["git", "-C", root, "push", "origin", branch]
        )
        if push is None or getattr(push, "returncode", 1) != 0:
            return self._fail(
                state,
                context,
                "publishing",
                "could not push the delivery branch",
                {"final_commit": final_commit, "pr_url": None},
            )

        canonical = context.get("canonical_repo")
        base = context.get("base_branch")
        existing = self._matching_pr(self._list_prs(canonical, branch, base), branch, base)
        if existing:
            return self._finish_published(state, context, final_commit, existing)

        title = "Plan: %s" % (
            context.get("title") or context.get("plan_id") or "cloud plan"
        )
        body = (
            "Cloud plan execution for `%s`.\n\n"
            "Delivery branch: `%s`\n"
            "Base branch: `%s`\n\n"
            "This pull request is delivered for human review and is not merged "
            "automatically."
            % (context.get("plan_id") or "plan", branch, base or "")
        )
        create = _run_git(
            self._runner,
            [
                "gh",
                "pr",
                "create",
                "--repo",
                canonical or "",
                "--base",
                base or "",
                "--head",
                branch,
                "--title",
                title,
                "--body",
                body,
            ],
        )
        url = ""
        if create is not None and getattr(create, "returncode", 1) == 0:
            stdout = (getattr(create, "stdout", "") or "").strip()
            if stdout:
                url = stdout.splitlines()[-1].strip()
        if not url:
            recovered = self._matching_pr(
                self._list_prs(canonical, branch, base), branch, base
            )
            if recovered:
                return self._finish_published(state, context, final_commit, recovered)
            return self._fail(
                state,
                context,
                "publishing",
                "could not create or reconcile a pull request",
                {"final_commit": final_commit, "pr_url": None},
            )
        return self._finish_published(state, context, final_commit, url)

    def _list_prs(self, canonical, branch, base):
        if not canonical:
            return []
        proc = _run_git(
            self._runner,
            [
                "gh",
                "pr",
                "list",
                "--repo",
                canonical,
                "--head",
                branch,
                "--base",
                base or "",
                "--state",
                "open",
                "--json",
                "url,state,headRefName,baseRefName",
            ],
        )
        if proc is None or getattr(proc, "returncode", 1) != 0:
            return []
        stdout = (getattr(proc, "stdout", "") or "").strip()
        if not stdout:
            return []
        try:
            data = json.loads(stdout)
        except ValueError:
            return []
        return data if isinstance(data, list) else []

    @staticmethod
    def _matching_pr(items, branch, base):
        for item in items or []:
            if not isinstance(item, dict):
                continue
            state = item.get("state")
            if state and str(state).upper() != "OPEN":
                continue
            head = item.get("headRefName")
            if head and head != branch:
                continue
            base_name = item.get("baseRefName")
            if base_name and base_name != base:
                continue
            url = item.get("url")
            if isinstance(url, str) and url:
                return url
        return None

    def _finish_published(self, state, context, final_commit, pr_url):
        now = self._clock()
        self.manager.update(
            context["job_id"],
            status="completed",
            stage="completed",
            final_commit=final_commit,
            pr_url=pr_url,
            error_message=None,
            finished_at=now,
            updated_at=now,
        )
        self._stop_server(state)
        return self.manager.inspect(context["job_id"])

    # ------------------------------------------------------------------
    # Failure and server ownership
    # ------------------------------------------------------------------
    def _record_failure(self, job_id, stage, message):
        try:
            self.manager.update(
                job_id,
                status="failed",
                stage=stage,
                error_message=message,
                finished_at=self._clock(),
                updated_at=self._clock(),
            )
        except Exception:  # noqa: BLE001 - the original failure still propagates
            pass

    def _fail(self, state, context, stage, message, extra=None):
        now = self._clock()
        fields = {
            "status": "failed",
            "stage": stage,
            "error_message": message,
            "finished_at": now,
            "updated_at": now,
        }
        if extra:
            fields.update(extra)
        self.manager.update(context["job_id"], **fields)
        self._stop_server(state)
        return self.manager.inspect(context["job_id"])

    def _stop_server(self, state):
        if state.get("stopped"):
            return
        state["stopped"] = True
        stop = state.get("stream_stop")
        if stop is not None:
            stop.set()
        server = state.get("server") or {}
        stop_server = server.get("stop")
        if callable(stop_server):
            try:
                stop_server()
            except Exception:  # noqa: BLE001 - stopping is best effort
                pass


def execute_job(manager, job_id, *, executor=None, poll_interval=2.0):
    """Drive one job, continuing the same executor across waiting outcomes."""
    task = manager.inspect(job_id)
    if task is None:
        raise PlanExecutionError("not_found", "unknown plan job")
    context = build_execution_context(manager, task)
    if executor is None:
        executor = PlanExecutor(manager)
    while True:
        result = executor.execute(context)
        status = result.get("status") if isinstance(result, dict) else None
        if status != "waiting_input":
            return result
        record = manager.inspect(job_id)
        if record is not None:
            if record.get("status") in TERMINAL_STATUSES:
                return record
        time.sleep(poll_interval)
