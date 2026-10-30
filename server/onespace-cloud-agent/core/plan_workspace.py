#!/usr/bin/env python3
"""Prepare a fresh, job-owned clone for a plan execution.

A plan job never reuses a shared checkout.  Each accepted job clones the
selected remote into a path derived from its own job id, fetches the chosen
base branch immediately before coding, records the full fetched commit and
checks out the job delivery branch at that commit.
"""

import os
import subprocess


class WorkspaceError(Exception):
    """A workspace preparation failure with its observable stage."""

    def __init__(self, stage, message):
        super().__init__(message)
        self.stage = stage
        self.message = message


def _run(runner, cmd, cwd=None):
    return runner(
        cmd,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )


def _detail(proc):
    stderr = (getattr(proc, "stderr", "") or "").strip()
    if not stderr:
        stderr = (getattr(proc, "stdout", "") or "").strip()
    return stderr or "command failed"


def _repository(task):
    repository = task.get("repository")
    if not isinstance(repository, dict):
        repository = {}
    remote = repository.get("remote_url") or task.get("remote_url")
    base = repository.get("base_branch") or task.get("base_branch")
    if not isinstance(remote, str) or not remote.strip():
        raise WorkspaceError("clone", "missing repository remote_url")
    if not isinstance(base, str) or not base.strip():
        raise WorkspaceError("fetch", "missing repository base_branch")
    return remote.strip(), base.strip()


def prepare_workspace(
    task,
    *,
    workspace_root,
    runner=subprocess.run,
    git_bin="git",
):
    """Clone, fetch and check out one job workspace.

    Returns the cloud clone path, the full fetched base commit and the unique
    delivery branch.  A recorded gitlink submodule boundary is initialized at
    its recorded commits only when ``.gitmodules`` exists in the clone.
    """
    job_id = task.get("id") or task.get("job_id")
    if not isinstance(job_id, str) or not job_id or "/" in job_id or job_id in (".", ".."):
        raise WorkspaceError("clone", "missing or unsafe job id")
    remote, base_branch = _repository(task)

    workspace = os.path.join(workspace_root, "plan-jobs", job_id, "workspace")
    workspace = os.path.abspath(workspace)
    if os.path.exists(workspace):
        raise WorkspaceError("clone", "workspace path already exists: %s" % workspace)
    os.makedirs(os.path.dirname(workspace), exist_ok=True)

    proc = _run(runner, [git_bin, "clone", "--quiet", "--no-checkout", remote, workspace])
    if getattr(proc, "returncode", 1) != 0:
        raise WorkspaceError("clone", _detail(proc))

    proc = _run(
        runner,
        [git_bin, "-C", workspace, "fetch", "--quiet", "origin", base_branch],
    )
    if getattr(proc, "returncode", 1) != 0:
        raise WorkspaceError("fetch", _detail(proc))

    proc = _run(runner, [git_bin, "-C", workspace, "rev-parse", "FETCH_HEAD"])
    if getattr(proc, "returncode", 1) != 0:
        raise WorkspaceError("fetch", _detail(proc))
    base_commit = (getattr(proc, "stdout", "") or "").strip()
    if not base_commit:
        raise WorkspaceError("fetch", "could not resolve fetched commit")

    delivery_branch = "ai-workflow/%s" % job_id
    proc = _run(
        runner,
        [
            git_bin,
            "-C",
            workspace,
            "checkout",
            "--quiet",
            "-B",
            delivery_branch,
            base_commit,
        ],
    )
    if getattr(proc, "returncode", 1) != 0:
        raise WorkspaceError("checkout", _detail(proc))

    if os.path.exists(os.path.join(workspace, ".gitmodules")):
        proc = _run(
            runner,
            [
                git_bin,
                "-C",
                workspace,
                "submodule",
                "update",
                "--init",
                "--recursive",
                "--quiet",
            ],
        )
        if getattr(proc, "returncode", 1) != 0:
            raise WorkspaceError("submodule", _detail(proc))

    return {
        "remote_root": workspace,
        "base_commit": base_commit,
        "delivery_branch": delivery_branch,
    }
