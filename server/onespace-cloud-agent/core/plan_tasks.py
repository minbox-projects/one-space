#!/usr/bin/env python3
"""Durable admission, projection and ownership state for cloud plan jobs.

Plan job state is owned by the cloud service, never by the repository or the
workflow plan tree.  Admission is serialized by a local OS file lock and every
mutation is written atomically so reconnecting clients observe one truthful
job per canonical repository plus frozen plan id.
"""

import base64
import binascii
import contextlib
import fcntl
import hashlib
import json
import logging
import os
import shlex
import signal
import subprocess
import sys
import threading
import time
import urllib.parse
import uuid

from core.plan_workspace import WorkspaceError, prepare_workspace

logger = logging.getLogger("onespace-agent.plan-tasks")

SCHEMA_VERSION = 1
TERMINAL_STATUSES = frozenset({"completed", "no_changes", "failed", "cancelled"})

DEFAULT_STATE_ROOT = "~/.config/onespace-cloud-agent/plan-tasks"
DEFAULT_WORKSPACE_ROOT = "/root/cloud-agent-workspaces"

_THREAD_LOCKS = {}
_THREAD_LOCKS_GUARD = threading.Lock()


class PlanError(Exception):
    """A handled plan admission or lookup failure with a stable code."""

    def __init__(self, code, message):
        super().__init__(message)
        self.code = code
        self.message = message


def _thread_lock(path):
    with _THREAD_LOCKS_GUARD:
        lock = _THREAD_LOCKS.get(path)
        if lock is None:
            lock = threading.Lock()
            _THREAD_LOCKS[path] = lock
        return lock


def _reject_json_constant(value):
    raise ValueError("invalid JSON constant: %s" % value)


def _strict_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key: %s" % key)
        result[key] = value
    return result


def _require(condition, message):
    if not condition:
        raise PlanError("invalid_payload", message)


def _safe_member(path):
    return (
        isinstance(path, str)
        and bool(path)
        and "\x00" not in path
        and not path.startswith("/")
        and "\\" not in path
        and not (_is_windows_absolute(path))
    )


def _is_windows_absolute(path):
    if len(path) >= 2 and path[1] == ":" and path[0].isalpha():
        return True
    return path.startswith("//")


def _validate_member_path(path):
    _require(_safe_member(path), "invalid plan file path: %r" % (path,))
    segments = path.split("/")
    _require(
        all(segment not in ("", ".", "..") for segment in segments),
        "invalid plan file path: %r" % (path,),
    )


def _validate_plan_id(plan_id):
    _require(isinstance(plan_id, str) and bool(plan_id), "missing plan_id")
    _require(
        "/" not in plan_id
        and "\\" not in plan_id
        and "\x00" not in plan_id
        and plan_id not in (".", ".."),
        "unsafe plan_id",
    )


def _normalize_envelope(envelope):
    _require(isinstance(envelope, dict), "payload must be a JSON object")
    schema_version = envelope.get("schema_version")
    _require(
        isinstance(schema_version, int)
        and not isinstance(schema_version, bool)
        and schema_version == SCHEMA_VERSION,
        "unsupported schema_version",
    )

    workspace = envelope.get("workspace")
    _require(isinstance(workspace, dict), "missing workspace")
    _require(
        isinstance(workspace.get("id"), str) and bool(workspace.get("id")),
        "missing workspace.id",
    )
    root_path = workspace.get("root_path", "")
    _require(isinstance(root_path, str), "workspace.root_path must be a string")

    repository = envelope.get("repository")
    _require(isinstance(repository, dict), "missing repository")
    remote_url = repository.get("remote_url")
    base_branch = repository.get("base_branch")
    _require(
        isinstance(remote_url, str) and bool(remote_url.strip()),
        "missing repository.remote_url",
    )
    _require(
        isinstance(base_branch, str) and bool(base_branch.strip()),
        "missing repository.base_branch",
    )

    plan = envelope.get("plan")
    _require(isinstance(plan, dict), "missing plan")
    plan_id = plan.get("plan_id")
    _validate_plan_id(plan_id)
    title = plan.get("title")
    _require(title is None or isinstance(title, str), "plan.title must be a string")
    if title is None:
        title = plan_id

    files = plan.get("files")
    _require(isinstance(files, list) and bool(files), "plan files must be non-empty")

    decoded = []
    seen = set()
    for entry in files:
        _require(isinstance(entry, dict), "plan file entry must be an object")
        path = entry.get("path")
        _validate_member_path(path)
        _require(path not in seen, "duplicate plan file path: %s" % path)
        seen.add(path)
        encoded = entry.get("content_base64")
        _require(isinstance(encoded, str), "plan file content_base64 must be a string")
        try:
            data = base64.b64decode(encoded, validate=True)
        except (binascii.Error, ValueError):
            raise PlanError("invalid_payload", "invalid base64 for %s" % path)
        decoded.append((path, data))

    request_id = envelope.get("request_id")
    _require(
        request_id is None
        or (isinstance(request_id, str) and bool(request_id.strip())),
        "invalid request_id",
    )

    return {
        "envelope": envelope,
        "workspace": workspace,
        "repository": repository,
        "base_branch": base_branch.strip(),
        "plan_id": plan_id,
        "title": title,
        "files": decoded,
        "request_id": request_id,
    }


def decode_submit_envelope(raw):
    """Decode and validate one submit payload without accepting any partial plan."""
    if isinstance(raw, (bytes, bytearray)):
        try:
            text = bytes(raw).decode("utf-8")
        except UnicodeDecodeError:
            raise PlanError("invalid_payload", "payload must be UTF-8")
    elif isinstance(raw, str):
        text = raw
    else:
        raise PlanError("invalid_payload", "payload must be bytes or text")

    try:
        envelope = json.loads(
            text,
            parse_constant=_reject_json_constant,
            object_pairs_hook=_strict_object,
        )
    except (ValueError, TypeError):
        raise PlanError("invalid_payload", "payload is not valid JSON")

    _normalize_envelope(envelope)
    return envelope


def _parse_remote(remote_url):
    """Return (host, owner, repository) for an SSH or HTTPS remote spelling."""
    if not isinstance(remote_url, str):
        return None
    value = remote_url.strip()
    if not value:
        return None
    host = ""
    path = ""
    if value.startswith("git@") or (
        "://" not in value and "@" in value and ":" in value.split("@", 1)[-1]
    ):
        authority, _, path = value.partition(":")
        host = authority.rsplit("@", 1)[-1]
    elif "://" in value:
        try:
            parsed = urllib.parse.urlsplit(value)
        except ValueError:
            return None
        host = parsed.hostname or ""
        path = parsed.path
    else:
        return None
    path = path.strip("/")
    if path.endswith(".git"):
        path = path[:-4]
    parts = [part for part in path.split("/") if part]
    if len(parts) < 2:
        return None
    return host.lower(), parts[-2], parts[-1]


def _payload_digest(files):
    digest = hashlib.sha256()
    for path, data in sorted(files, key=lambda item: item[0]):
        digest.update(path.encode("utf-8"))
        digest.update(b"\x00")
        digest.update(data)
    return digest.hexdigest()


def _default_workspace_root(config):
    plan_cfg = config.get("plan_jobs") if isinstance(config, dict) else None
    if isinstance(plan_cfg, dict) and plan_cfg.get("workspace_root"):
        return os.path.expanduser(str(plan_cfg["workspace_root"]))
    worker_cfg = config.get("issue_worker") if isinstance(config, dict) else None
    if isinstance(worker_cfg, dict) and worker_cfg.get("workspace_root"):
        return os.path.expanduser(str(worker_cfg["workspace_root"]))
    return DEFAULT_WORKSPACE_ROOT


def _timestamp_value(value):
    try:
        return float(value)
    except (TypeError, ValueError):
        return 0.0


def _projection(record):
    return {
        "id": record.get("id"),
        "source": "plan",
        "repo": record.get("repo"),
        "issue_number": None,
        "title": record.get("plan_title"),
        "status": record.get("status"),
        "started_at": record.get("started_at"),
        "finished_at": record.get("finished_at"),
        "duration_seconds": record.get("duration_seconds"),
        "branch": record.get("branch"),
        "pr_url": record.get("pr_url"),
        "error_message": record.get("error_message"),
        "author": None,
        "trigger_type": "plan",
        "attempts": record.get("attempts", 0),
        "max_attempts": None,
        "diff_stat": record.get("diff_stat"),
        "step": record.get("step"),
        "plan_id": record.get("plan_id"),
        "plan_title": record.get("plan_title"),
        "workspace_id": record.get("workspace_id"),
        "base_branch": record.get("base_branch"),
        "base_commit": record.get("base_commit"),
        "final_commit": record.get("final_commit"),
        "session_id": record.get("session_id"),
        "session_url": record.get("session_url"),
        "stage": record.get("stage"),
        "question": record.get("question"),
        "decision": record.get("decision"),
        "remote_root": record.get("remote_root"),
        "local_root": record.get("local_root"),
        "updated_at": record.get("updated_at"),
    }


class PlanTaskManager:
    """Own plan-job admission, durable projections and owned cancellation."""

    def __init__(
        self,
        config,
        *,
        state_root=None,
        workspace_root=None,
        gh_runner=None,
        tmux_runner=None,
        clock=time.time,
        agent_dir=None,
        config_path=None,
    ):
        self.config = config if isinstance(config, dict) else {}
        self.state_root = os.path.abspath(
            os.path.expanduser(state_root or DEFAULT_STATE_ROOT)
        )
        self.workspace_root = os.path.abspath(
            os.path.expanduser(workspace_root or _default_workspace_root(self.config))
        )
        self._gh_runner = gh_runner or subprocess.run
        self._tmux_runner = tmux_runner or subprocess.run
        self._clock = clock or time.time
        self.agent_dir = os.path.abspath(
            agent_dir or os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        )
        self.config_path = os.path.abspath(config_path) if config_path else None
        self._last_controller_error = None

    # ------------------------------------------------------------------
    # Decoding
    # ------------------------------------------------------------------
    def decode_submit_envelope(self, raw):
        return decode_submit_envelope(raw)

    # ------------------------------------------------------------------
    # Admission
    # ------------------------------------------------------------------
    def submit(self, envelope, request_id=None):
        """Accept one plan payload or return the existing job for its identity."""
        try:
            if isinstance(envelope, (bytes, bytearray, str)):
                decoded = decode_submit_envelope(envelope)
            else:
                decoded = envelope
            normalized = _normalize_envelope(decoded)
            request = request_id or normalized["request_id"]
            _require(
                isinstance(request, str) and bool(request.strip()),
                "missing request_id",
            )
            canonical, _host = self._resolve_identity(
                normalized["repository"]["remote_url"]
            )
        except PlanError as exc:
            return {"ok": False, "code": exc.code, "error": exc.message}
        except Exception as exc:  # noqa: BLE001 - submit must not raise
            logger.exception("plan submit failed")
            return {"ok": False, "code": "internal_error", "error": str(exc)}

        digest = _payload_digest(normalized["files"])
        identity = "%s\n%s" % (canonical.lower(), normalized["plan_id"])

        try:
            with self._locked():
                requests = self._read_requests()
                existing_id = requests.get(request)
                if existing_id:
                    existing = self._read_record(existing_id)
                    if existing is not None:
                        return {
                            "ok": True,
                            "reused": True,
                            "task": _projection(existing),
                        }

                index = self._read_index()
                existing_id = index.get(identity)
                if existing_id:
                    existing = self._read_record(existing_id)
                    if existing is not None:
                        if (
                            existing.get("base_branch") != normalized["base_branch"]
                            or existing.get("payload_digest") != digest
                        ):
                            return {
                                "ok": False,
                                "code": "identity_conflict",
                                "error": (
                                    "repository and plan already accepted with "
                                    "different base branch or plan bytes"
                                ),
                            }
                        requests[request] = existing_id
                        self._write_requests(requests)
                        return {
                            "ok": True,
                            "reused": True,
                            "task": _projection(existing),
                        }

                job_id = self._new_job_id()
                session_name = "plan-%s" % job_id
                now = self._clock()
                record = {
                    "schema_version": SCHEMA_VERSION,
                    "id": job_id,
                    "identity": identity,
                    "canonical_repo": canonical,
                    "repo": canonical,
                    "repository": normalized["repository"],
                    "workspace": normalized["workspace"],
                    "plan_id": normalized["plan_id"],
                    "plan_title": normalized["title"],
                    "base_branch": normalized["base_branch"],
                    "payload_digest": digest,
                    "status": "accepted",
                    "stage": "accepted",
                    "started_at": now,
                    "finished_at": None,
                    "duration_seconds": None,
                    "branch": "ai-workflow/%s" % job_id,
                    "pr_url": None,
                    "error_message": None,
                    "attempts": 0,
                    "diff_stat": None,
                    "step": None,
                    "workspace_id": normalized["workspace"].get("id"),
                    "base_commit": None,
                    "final_commit": None,
                    "session_id": None,
                    "session_url": None,
                    "question": None,
                    "decision": None,
                    "remote_root": None,
                    "local_root": normalized["workspace"].get("root_path", ""),
                    "controller_session": session_name,
                    "updated_at": now,
                }
                self._persist_new_job(
                    job_id,
                    record,
                    normalized,
                    request,
                    canonical,
                )

                index[identity] = job_id
                requests[request] = job_id
                self._write_index(index)
                self._write_requests(requests)

                self._spawn_controller(job_id, session_name)
                return {
                    "ok": True,
                    "reused": False,
                    "task": _projection(record),
                }
        except PlanError as exc:
            return {"ok": False, "code": exc.code, "error": exc.message}
        except Exception as exc:  # noqa: BLE001 - submit must not raise
            logger.exception("plan admission failed")
            return {"ok": False, "code": "internal_error", "error": str(exc)}

    def _resolve_identity(self, remote_url):
        parsed = _parse_remote(remote_url)
        cmd = ["gh", "repo", "view", remote_url, "--json", "nameWithOwner"]
        try:
            proc = self._gh_runner(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False,
            )
        except Exception as exc:  # noqa: BLE001 - an unresolved identity is a refusal
            raise PlanError("unresolved_identity", "could not resolve repository: %s" % exc)
        if getattr(proc, "returncode", 1) != 0:
            detail = (getattr(proc, "stderr", "") or "").strip()
            raise PlanError(
                "unresolved_identity",
                "could not resolve repository identity%s"
                % (": %s" % detail if detail else ""),
            )
        try:
            data = json.loads(getattr(proc, "stdout", "") or "")
        except (TypeError, ValueError):
            raise PlanError("unresolved_identity", "invalid gh repository response")
        name = data.get("nameWithOwner") if isinstance(data, dict) else None
        if not isinstance(name, str) or "/" not in name:
            raise PlanError("unresolved_identity", "gh did not return nameWithOwner")
        return name, (parsed[0] if parsed else "")

    def _new_job_id(self):
        while True:
            candidate = "plan-%d-%s" % (int(self._clock()), uuid.uuid4().hex[:12])
            if not os.path.exists(os.path.join(self.state_root, candidate)):
                return candidate

    def _persist_new_job(self, job_id, record, normalized, request, canonical):
        job_dir = os.path.join(self.state_root, job_id)
        os.makedirs(job_dir, exist_ok=False)
        upload_dir = os.path.join(job_dir, "upload")
        for path, data in normalized["files"]:
            destination = os.path.join(upload_dir, *path.split("/"))
            os.makedirs(os.path.dirname(destination), exist_ok=True)
            self._write_bytes(destination, data)
        request_payload = {
            "schema_version": SCHEMA_VERSION,
            "request_id": request,
            "canonical_repo": canonical,
            "envelope": normalized["envelope"],
        }
        self._write_json(os.path.join(job_dir, "request.json"), request_payload)
        self._write_json(os.path.join(job_dir, "task.json"), record)

    # ------------------------------------------------------------------
    # Controller
    # ------------------------------------------------------------------
    def _controller_command(self, job_id):
        argv = [sys.executable, "agent.py", "plan", "run", job_id]
        if self.config_path:
            argv.extend(["--config", self.config_path])
        argv.append("--json")
        return "cd %s && %s" % (
            shlex.quote(self.agent_dir),
            " ".join(shlex.quote(part) for part in argv),
        )

    def _spawn_controller(self, job_id, session_name):
        command = self._controller_command(job_id)
        cmd = ["tmux", "new-session", "-d", "-s", session_name, command]
        try:
            proc = self._tmux_runner(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False,
            )
            if getattr(proc, "returncode", 0) != 0:
                self._last_controller_error = (
                    getattr(proc, "stderr", "") or "tmux new-session failed"
                ).strip()
            else:
                self._last_controller_error = None
        except Exception as exc:  # noqa: BLE001 - durable job remains inspectable
            self._last_controller_error = str(exc)
            logger.warning("could not start plan controller for %s: %s", job_id, exc)

    def _session_alive(self, session_name):
        cmd = ["tmux", "has-session", "-t", session_name]
        try:
            proc = self._tmux_runner(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False,
            )
        except Exception:  # noqa: BLE001 - a missing tmux is not a live session
            return False
        return getattr(proc, "returncode", 1) == 0

    def _kill_session(self, session_name):
        cmd = ["tmux", "kill-session", "-t", session_name]
        try:
            self._tmux_runner(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False,
            )
        except Exception:  # noqa: BLE001 - cancellation is best effort
            pass

    # ------------------------------------------------------------------
    # Execution preparation
    # ------------------------------------------------------------------
    def run_job(self, job_id):
        """Prepare the job workspace and hand it to the coding lifecycle."""
        if not self._safe_job_id(job_id):
            raise PlanError("not_found", "unknown plan job")
        with self._locked():
            record = self._read_record(job_id)
            if record is None:
                raise PlanError("not_found", "unknown plan job")
            if record.get("status") in TERMINAL_STATUSES:
                return _projection(record)
            record["status"] = "preparing"
            record["stage"] = "preparing"
            record["updated_at"] = self._clock()
            self._write_record(record)
            repository = dict(record.get("repository") or {})
            plan_id = record.get("plan_id")

        try:
            prepared = prepare_workspace(
                {"id": job_id, "repository": repository},
                workspace_root=self.workspace_root,
            )
        except WorkspaceError as exc:
            return self._fail(job_id, exc.stage, exc.message)
        except Exception as exc:  # noqa: BLE001 - preparation failure is projected
            return self._fail(job_id, "workspace", str(exc))

        try:
            self._copy_plan_upload(job_id, plan_id, prepared["remote_root"])
        except Exception as exc:  # noqa: BLE001 - copy failure is projected
            return self._fail(job_id, "upload", str(exc))

        with self._locked():
            record = self._read_record(job_id)
            if record is None:
                raise PlanError("not_found", "unknown plan job")
            if record.get("status") in TERMINAL_STATUSES:
                return _projection(record)
            now = self._clock()
            record.update(
                {
                    "remote_root": prepared["remote_root"],
                    "base_commit": prepared["base_commit"],
                    "branch": prepared["delivery_branch"],
                    "status": "running",
                    "stage": "coding_pending",
                    "updated_at": now,
                }
            )
            record["duration_seconds"] = self._duration(record, now)
            self._write_record(record)
            prepared_projection = _projection(record)

        try:
            from core import plan_execution
        except ImportError:
            return prepared_projection
        execute_job = getattr(plan_execution, "execute_job", None)
        if execute_job is None:
            return prepared_projection
        try:
            result = execute_job(self, job_id)
        except Exception as exc:  # noqa: BLE001 - execution failure is projected
            stage = getattr(exc, "stage", None) or "execution"
            message = getattr(exc, "message", None) or str(exc) or "execution failed"
            return self._fail(job_id, stage, message)
        return result if result is not None else self.inspect(job_id)

    def _copy_plan_upload(self, job_id, plan_id, remote_root):
        source = os.path.join(self.state_root, job_id, "upload")
        destination = os.path.join(
            remote_root, ".ai-workflow", "plans", plan_id
        )
        if not os.path.isdir(source):
            raise PlanError("invalid_payload", "accepted upload is missing")
        for root, _dirs, names in os.walk(source):
            relative = os.path.relpath(root, source)
            target_root = (
                destination
                if relative == "."
                else os.path.join(destination, relative)
            )
            os.makedirs(target_root, exist_ok=True)
            for name in names:
                source_path = os.path.join(root, name)
                target_path = os.path.join(target_root, name)
                with open(source_path, "rb") as handle:
                    data = handle.read()
                self._write_bytes(target_path, data)

    def _fail(self, job_id, stage, message):
        with self._locked():
            record = self._read_record(job_id)
            if record is None:
                raise PlanError("not_found", "unknown plan job")
            now = self._clock()
            record.update(
                {
                    "status": "failed",
                    "stage": stage,
                    "error_message": message,
                    "finished_at": now,
                    "updated_at": now,
                }
            )
            record["duration_seconds"] = self._duration(record, now)
            self._write_record(record)
            return _projection(record)

    # ------------------------------------------------------------------
    # Projections
    # ------------------------------------------------------------------
    def update(self, job_id, **fields):
        """Atomically merge lifecycle fields and return the projection."""
        if not self._safe_job_id(job_id):
            return None
        with self._locked():
            record = self._read_record(job_id)
            if record is None:
                return None
            record.update(fields)
            now = self._clock()
            record["updated_at"] = now
            record["duration_seconds"] = self._duration(record, now)
            self._write_record(record)
            return _projection(record)

    def get_request(self, job_id):
        """Return the stored submit request metadata and envelope."""
        if not self._safe_job_id(job_id):
            return {}
        path = self._job_path(job_id, "request.json")
        try:
            with open(path, "r", encoding="utf-8") as handle:
                data = json.load(handle)
        except (OSError, ValueError):
            return {}
        return data if isinstance(data, dict) else {}

    def record_decision(self, job_id, question, answer, basis):
        """Append one automatic decision and persist a concise summary."""
        if not self._safe_job_id(job_id):
            return False
        with self._locked():
            record = self._read_record(job_id)
            if record is None:
                return False
            decisions = record.get("decisions")
            if not isinstance(decisions, list):
                decisions = []
            now = self._clock()
            entry = {
                "question": question,
                "answer": answer,
                "basis": basis,
                "recorded_at": now,
            }
            decisions.append(entry)
            record["decisions"] = decisions
            record["decision"] = entry
            record["updated_at"] = now
            record["duration_seconds"] = self._duration(record, now)
            self._write_record(record)
            return True

    def inspect(self, job_id):
        if not self._safe_job_id(job_id):
            return None
        with self._locked():
            record = self._read_record(job_id)
            return _projection(record) if record is not None else None

    def list_tasks(self):
        if not os.path.isdir(self.state_root):
            return []
        records = []
        for name in os.listdir(self.state_root):
            if name.startswith("."):
                continue
            record = self._read_record(name)
            if record is not None:
                records.append(record)
        records.sort(
            key=lambda item: (
                _timestamp_value(item.get("started_at")),
                str(item.get("id") or ""),
            ),
            reverse=True,
        )

        projections = []
        for record in records:
            if (
                record.get("status") not in TERMINAL_STATUSES
                and record.get("controller_session")
                and not self._session_alive(record["controller_session"])
            ):
                record = self._mark_interrupted(record)
            projections.append(_projection(record))
        return projections

    def _mark_interrupted(self, record):
        with self._locked():
            current = self._read_record(record.get("id"))
            if current is None:
                return record
            if current.get("status") in TERMINAL_STATUSES:
                return current
            now = self._clock()
            current.update(
                {
                    "status": "failed",
                    "stage": "interrupted",
                    "error_message": (
                        "controller session is no longer running; "
                        "job interrupted and was not rerun"
                    ),
                    "finished_at": now,
                    "updated_at": now,
                }
            )
            current["duration_seconds"] = self._duration(current, now)
            self._write_record(current)
            return current

    def cancel(self, job_id):
        if not self._safe_job_id(job_id):
            return {"ok": False, "code": "not_found", "error": "unknown plan job"}
        with self._locked():
            record = self._read_record(job_id)
            if record is None:
                return {"ok": False, "code": "not_found", "error": "unknown plan job"}
            if record.get("status") in TERMINAL_STATUSES:
                return {"ok": True, "task": _projection(record)}
            session_name = record.get("controller_session")
            if session_name:
                self._kill_session(session_name)
            server_pid = record.get("server_pid")
            if isinstance(server_pid, int) and not isinstance(server_pid, bool) and server_pid > 0:
                try:
                    os.killpg(os.getpgid(server_pid), signal.SIGTERM)
                except OSError:
                    pass
                record["server_pid"] = None
            now = self._clock()
            record.update(
                {
                    "status": "cancelled",
                    "stage": "cancelled",
                    "finished_at": now,
                    "updated_at": now,
                }
            )
            record["duration_seconds"] = self._duration(record, now)
            self._write_record(record)
            return {"ok": True, "task": _projection(record)}

    def aggregate_tasks(self, issue_tasks):
        combined = list(issue_tasks or [])
        combined.extend(self.list_tasks())
        combined.sort(
            key=lambda item: _timestamp_value(item.get("started_at")),
            reverse=True,
        )
        return combined

    # ------------------------------------------------------------------
    # Persistence primitives
    # ------------------------------------------------------------------
    @contextlib.contextmanager
    def _locked(self):
        os.makedirs(self.state_root, exist_ok=True)
        lock_path = os.path.join(self.state_root, ".lock")
        with _thread_lock(os.path.abspath(self.state_root)):
            handle = open(lock_path, "a+")
            try:
                fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
                yield
            finally:
                try:
                    fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
                finally:
                    handle.close()

    @staticmethod
    def _safe_job_id(job_id):
        return (
            isinstance(job_id, str)
            and bool(job_id)
            and "/" not in job_id
            and "\\" not in job_id
            and job_id not in (".", "..")
        )

    def _job_path(self, job_id, *parts):
        return os.path.join(self.state_root, job_id, *parts)

    def _read_record(self, job_id):
        if not self._safe_job_id(job_id):
            return None
        path = self._job_path(job_id, "task.json")
        if not os.path.isfile(path):
            return None
        try:
            with open(path, "r", encoding="utf-8") as handle:
                record = json.load(handle)
        except (OSError, ValueError):
            return None
        if not isinstance(record, dict):
            return None
        return record

    def _write_record(self, record):
        job_id = record.get("id")
        if not self._safe_job_id(job_id):
            raise PlanError("not_found", "unknown plan job")
        self._write_json(self._job_path(job_id, "task.json"), record)

    def _read_index(self):
        path = os.path.join(self.state_root, "index.json")
        try:
            with open(path, "r", encoding="utf-8") as handle:
                data = json.load(handle)
        except (OSError, ValueError):
            return {}
        identities = data.get("identities") if isinstance(data, dict) else None
        return dict(identities) if isinstance(identities, dict) else {}

    def _write_index(self, identities):
        self._write_json(
            os.path.join(self.state_root, "index.json"),
            {"schema_version": SCHEMA_VERSION, "identities": identities},
        )

    def _read_requests(self):
        path = os.path.join(self.state_root, "requests.json")
        try:
            with open(path, "r", encoding="utf-8") as handle:
                data = json.load(handle)
        except (OSError, ValueError):
            return {}
        requests = data.get("requests") if isinstance(data, dict) else None
        return dict(requests) if isinstance(requests, dict) else {}

    def _write_requests(self, requests):
        self._write_json(
            os.path.join(self.state_root, "requests.json"),
            {"schema_version": SCHEMA_VERSION, "requests": requests},
        )

    def _write_json(self, path, payload):
        data = json.dumps(payload, ensure_ascii=False, indent=2).encode("utf-8")
        self._write_bytes(path, data)

    def _write_bytes(self, path, data):
        directory = os.path.dirname(path)
        os.makedirs(directory, exist_ok=True)
        temporary = "%s.tmp-%d-%s" % (path, os.getpid(), uuid.uuid4().hex[:8])
        try:
            with open(temporary, "wb") as handle:
                handle.write(data)
                handle.flush()
                os.fsync(handle.fileno())
            os.replace(temporary, path)
        finally:
            if os.path.exists(temporary):
                try:
                    os.remove(temporary)
                except OSError:
                    pass
        try:
            directory_fd = os.open(directory, os.O_RDONLY)
        except OSError:
            return
        try:
            os.fsync(directory_fd)
        except OSError:
            pass
        finally:
            os.close(directory_fd)

    @staticmethod
    def _duration(record, now):
        try:
            return float(now) - float(record.get("started_at"))
        except (TypeError, ValueError):
            return None
