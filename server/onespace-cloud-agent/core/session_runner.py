#!/usr/bin/env python3
"""
onespace-cloud-agent: 长程编程与重构会话管理器 (Session Runner)
基于 tmux 实现断网保活、Mac 合盖即走、随时开盖恢复现场。
"""

import json
import logging
import os
import re
import shlex
import shutil
import subprocess
import sys
import time
from typing import Any, Dict, List, Optional

logger = logging.getLogger("onespace-session-runner")

FALLBACK_OPENCODE_BIN = "/root/.opencode/bin/opencode"
TMUX_PANE_FORMAT = "#{pane_current_path}|#{session_name}|#{pane_id}"
TMUX_SESSION_PANE_FORMAT = (
    "#{session_name}|#{session_created}|#{session_windows}|"
    "#{pane_current_path}|#{pane_pid}"
)


def _parse_worktree_porcelain(output: str) -> List[Dict[str, str]]:
    """解析 ``git worktree list --porcelain`` 输出。"""
    worktrees: List[Dict[str, str]] = []
    current: Dict[str, str] = {}
    for raw_line in (output or "").splitlines():
        line = raw_line.strip()
        if not line:
            if current:
                worktrees.append(current)
                current = {}
            continue
        key, _, value = line.partition(" ")
        if key == "worktree":
            if current:
                worktrees.append(current)
            current = {"path": value, "branch": "", "head": ""}
        elif key == "HEAD":
            current["head"] = value
        elif key == "branch":
            if value.startswith("refs/heads/"):
                value = value[len("refs/heads/"):]
            current["branch"] = value
        elif key in ("locked", "prunable", "bare", "detached"):
            current[key] = value or "1"
    if current:
        worktrees.append(current)
    return worktrees


def list_worktree_registrations(repo_dir: str) -> Optional[List[Dict[str, str]]]:
    """返回 Git 注册的 Worktree；无法检查时返回 ``None``。"""
    try:
        res = subprocess.run(
            ["git", "-C", repo_dir, "worktree", "list", "--porcelain"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
    except Exception as exc:
        logger.warning("检查 Git Worktree 注册信息失败: %s", exc)
        return None
    if res.returncode != 0:
        logger.warning(
            "检查 Git Worktree 注册信息失败: %s", (res.stderr or "").strip()
        )
        return None
    return _parse_worktree_porcelain(res.stdout or "")


def _parse_pane_lines(
    output: str, path_index: int, session_index: int, pane_index: int
) -> List[Dict[str, str]]:
    panes: List[Dict[str, str]] = []
    for line in (output or "").splitlines():
        parts = line.split("|")
        if len(parts) <= max(path_index, session_index, pane_index):
            continue
        pane_path = parts[path_index].strip()
        if not pane_path:
            continue
        panes.append(
            {
                "path": pane_path,
                "session": parts[session_index].strip(),
                "pane_id": parts[pane_index].strip(),
            }
        )
    return panes


def collect_tmux_panes() -> Optional[List[Dict[str, str]]]:
    """检查所有 tmux 面板的工作目录；完全无法检查时返回 ``None``。"""
    try:
        res = subprocess.run(
            ["tmux", "list-panes", "-a", "-F", TMUX_PANE_FORMAT],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
    except Exception:
        res = None
    if res is not None and res.returncode == 0:
        panes = _parse_pane_lines(res.stdout or "", 0, 1, 2)
        if panes:
            return panes
    try:
        res = subprocess.run(
            ["tmux", "list-sessions", "-F", TMUX_SESSION_PANE_FORMAT],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
    except Exception:
        return None
    if res.returncode != 0:
        return None
    return _parse_pane_lines(res.stdout or "", 3, 0, 4)


def _find_registration(
    registrations: List[Dict[str, str]], path: str
) -> Optional[Dict[str, str]]:
    target = os.path.abspath(path)
    for entry in registrations:
        if os.path.abspath(entry.get("path", "")) == target:
            return entry
    return None


def _path_within(path: str, root: str) -> bool:
    candidate = os.path.abspath(path)
    base = os.path.abspath(root)
    return candidate == base or candidate.startswith(base + os.sep)


def worktree_is_clean(path: str) -> Optional[bool]:
    """返回 Worktree 是否干净（含未跟踪文件）；无法检查时返回 ``None``。"""
    try:
        res = subprocess.run(
            [
                "git",
                "-C",
                path,
                "status",
                "--porcelain",
                "--untracked-files=normal",
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
    except Exception as exc:
        logger.warning("检查 Worktree 状态失败 %s: %s", path, exc)
        return None
    if res.returncode != 0:
        logger.warning(
            "检查 Worktree 状态失败 %s: %s", path, (res.stderr or "").strip()
        )
        return None
    return not (res.stdout or "").strip()


def remove_registered_worktree(main_repo_dir: str, worktree_path: str) -> bool:
    """使用非强制 Git 移除 Worktree，保留分支。"""
    try:
        res = subprocess.run(
            ["git", "-C", main_repo_dir, "worktree", "remove", worktree_path],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
    except Exception as exc:
        logger.warning("移除 Worktree 失败 %s: %s", worktree_path, exc)
        return False
    if res.returncode != 0:
        logger.warning(
            "移除 Worktree 失败 %s: %s", worktree_path, (res.stderr or "").strip()
        )
        return False
    return True


def find_worktree_root(pane_path: str, workspace_root: str) -> Optional[str]:
    """把面板路径（可位于子目录）解析为 ``<workspace_root>/*-worktrees/<branch>``。"""
    if not pane_path:
        return None
    base = os.path.abspath(workspace_root)
    pane = os.path.abspath(pane_path)
    try:
        relative = os.path.relpath(pane, base)
    except ValueError:
        return None
    if relative == os.pardir or relative.startswith(os.pardir + os.sep):
        return None
    parts = relative.split(os.sep)
    if len(parts) < 2 or not parts[0].endswith("-worktrees") or not parts[1]:
        return None
    return os.path.join(base, parts[0], parts[1])


def _is_managed_worktree_root(path: str, workspace_root: str) -> bool:
    base = os.path.abspath(path)
    parent = os.path.dirname(base)
    return (
        os.path.dirname(parent) == os.path.abspath(workspace_root)
        and os.path.basename(parent).endswith("-worktrees")
    )


def cleanup_registered_worktree(
    worktree_path: str,
    workspace_root: str,
    active_pane_paths: List[str],
    registrations: Optional[List[Dict[str, str]]] = None,
) -> bool:
    """在注册、干净、无活动面板且位于受管根目录时移除 Worktree。"""
    if not _is_managed_worktree_root(worktree_path, workspace_root):
        logger.info("保留受管根目录之外的 Worktree: %s", worktree_path)
        return False
    if any(_path_within(pane, worktree_path) for pane in active_pane_paths if pane):
        logger.info("保留仍被 tmux 面板使用的 Worktree: %s", worktree_path)
        return False
    if registrations is None:
        registrations = list_worktree_registrations(worktree_path)
    if registrations is None:
        logger.warning("无法检查 Worktree 注册信息，已保留: %s", worktree_path)
        return False
    if _find_registration(registrations, worktree_path) is None:
        logger.info("保留未注册的 Worktree 目录: %s", worktree_path)
        return False
    clean = worktree_is_clean(worktree_path)
    if clean is None:
        logger.warning("无法检查 Worktree 状态，已保留: %s", worktree_path)
        return False
    if not clean:
        logger.info("保留存在未提交改动的 Worktree: %s", worktree_path)
        return False
    main_repo = registrations[0].get("path", "") if registrations else ""
    return remove_registered_worktree(main_repo or worktree_path, worktree_path)


def sweep_stale_worktrees(
    workspace_root: str, retention_days: int, now: Optional[float] = None
) -> None:
    """清理超过保留期、已注册、干净且无 tmux 面板使用的 Worktree。"""
    base = os.path.abspath(workspace_root)
    if not os.path.isdir(base):
        return
    cutoff = (time.time() if now is None else now) - (retention_days * 86400)
    panes = collect_tmux_panes()
    if panes is None:
        logger.warning("无法检查 tmux 面板，跳过过期 Worktree 清理")
        return
    active_paths = [pane.get("path", "") for pane in panes]
    try:
        entries = sorted(os.listdir(base))
    except OSError as exc:
        logger.warning("列出工作区失败 %s: %s", base, exc)
        return
    for entry in entries:
        if not entry.endswith("-worktrees"):
            continue
        worktree_base = os.path.join(base, entry)
        if os.path.islink(worktree_base) or not os.path.isdir(worktree_base):
            continue
        try:
            branch_dirs = sorted(os.listdir(worktree_base))
        except OSError as exc:
            logger.warning("列出 Worktree 目录失败 %s: %s", worktree_base, exc)
            continue
        for branch_dir in branch_dirs:
            candidate = os.path.join(worktree_base, branch_dir)
            if os.path.islink(candidate) or not os.path.isdir(candidate):
                continue
            try:
                mtime = os.path.getmtime(candidate)
            except OSError as exc:
                logger.warning("检查 Worktree 时间失败 %s: %s", candidate, exc)
                continue
            if mtime >= cutoff:
                continue
            try:
                cleanup_registered_worktree(candidate, base, active_paths)
            except Exception as exc:
                logger.warning("清理过期 Worktree 失败 %s: %s", candidate, exc)


class SessionRunner:
    def __init__(self, workspace_root: str = "/root/cloud-agent-workspaces"):
        self.workspace_root = workspace_root
        os.makedirs(self.workspace_root, exist_ok=True)

    def _sanitize_name(self, name: str) -> str:
        return re.sub(r"[^a-zA-Z0-9_\-]", "_", name)

    def list_sessions(self) -> List[str]:
        """列出所有活跃的 tmux 会话名称"""
        try:
            res = subprocess.run(
                ["tmux", "list-sessions", "-F", "#{session_name}"],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False
            )
            if res.returncode != 0:
                return []
            return [line.strip() for line in res.stdout.strip().splitlines() if line.strip()]
        except Exception:
            return []

    def list_sessions_detailed(self) -> List[Dict[str, Any]]:
        """获取所有活跃会话的详细信息（结构化）"""
        try:
            res = subprocess.run(
                ["tmux", "list-sessions", "-F", "#{session_name}|#{session_created}|#{session_windows}|#{pane_current_path}|#{pane_pid}"],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False
            )
            if res.returncode != 0:
                return []
            results = []
            for line in res.stdout.strip().splitlines():
                parts = line.strip().split("|")
                if not parts or not parts[0]:
                    continue
                session_name = parts[0]
                created_at = int(parts[1]) if len(parts) > 1 and parts[1].isdigit() else int(time.time())
                windows_count = int(parts[2]) if len(parts) > 2 and parts[2].isdigit() else 1
                working_dir = parts[3] if len(parts) > 3 else ""
                pane_pid = int(parts[4]) if len(parts) > 4 and parts[4].isdigit() else 0

                repo_name = ""
                branch = ""
                if session_name.startswith("dev_"):
                    stripped = session_name[4:]
                    if "_" in stripped:
                        repo_name, branch = stripped.split("_", 1)
                    else:
                        repo_name = stripped
                else:
                    repo_name = session_name

                worktree_name = ""
                if "-worktrees/" in working_dir:
                    parts = working_dir.split("-worktrees/")
                    if len(parts) > 1:
                        worktree_name = parts[1].strip("/")
                elif working_dir:
                    worktree_name = os.path.basename(working_dir.rstrip("/"))

                opencode_session_id = self._resolve_opencode_session_id(working_dir)

                results.append({
                    "session_name": session_name,
                    "repo_name": repo_name,
                    "branch": branch,
                    "working_dir": working_dir,
                    "worktree_name": worktree_name,
                    "opencode_session_id": opencode_session_id,
                    "created_at": created_at,
                    "windows_count": windows_count,
                    "pane_pid": pane_pid,
                    "is_active": True
                })
            return results
        except Exception:
            return []

    def _resolve_opencode_session_id(self, working_dir: str) -> Optional[str]:
        """尝试从 OpenCode 运行时数据库中查找对应工作目录的最新会话 ID"""
        if not working_dir:
            return None
        db_candidates = [
            os.path.expanduser("~/.local/share/opencode/opencode.db"),
            "/root/.local/share/opencode/opencode.db",
            os.path.expanduser("~/.opencode/opencode.db"),
        ]
        norm_target = os.path.realpath(working_dir)
        for db_path in db_candidates:
            if not os.path.exists(db_path):
                continue
            try:
                import sqlite3
                conn = sqlite3.connect(db_path, timeout=1.0)
                cursor = conn.cursor()
                for table in ("session_v2", "session"):
                    try:
                        cursor.execute(
                            f"SELECT id, directory FROM {table} ORDER BY time_updated DESC LIMIT 50"
                        )
                        rows = cursor.fetchall()
                        for row in rows:
                            if not row or not row[0]:
                                continue
                            db_dir = row[1] if len(row) > 1 and row[1] else ""
                            if db_dir and (db_dir == working_dir or os.path.realpath(db_dir) == norm_target):
                                conn.close()
                                return str(row[0])
                    except Exception:
                        pass
                conn.close()
            except Exception:
                pass
        return None

    def _get_last_commit(self, path: str) -> Optional[Dict[str, Any]]:
        try:
            res = subprocess.run(
                ["git", "-C", path, "log", "-1", "--format=%h|%an|%at|%s"],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False
            )
            if res.returncode == 0 and res.stdout.strip():
                parts = res.stdout.strip().split("|", 3)
                if len(parts) == 4:
                    return {
                        "hash": parts[0],
                        "author": parts[1],
                        "timestamp": int(parts[2]) if parts[2].isdigit() else 0,
                        "message": parts[3]
                    }
        except Exception:
            pass
        return None

    def list_worktrees_detailed(self) -> List[Dict[str, Any]]:
        """列出所有已创建的 Git Worktree 及其状态"""
        active_sessions = {s["session_name"]: s for s in self.list_sessions_detailed()}
        worktrees = []
        if not os.path.exists(self.workspace_root):
            return []

        for entry in sorted(os.listdir(self.workspace_root)):
            entry_path = os.path.join(self.workspace_root, entry)
            if not os.path.isdir(entry_path):
                continue

            # 模式 1: <repo_name>-worktrees/<branch>
            if entry.endswith("-worktrees"):
                repo_name = entry[:-10]
                try:
                    for branch_dir in sorted(os.listdir(entry_path)):
                        wt_dir = os.path.join(entry_path, branch_dir)
                        if not os.path.isdir(wt_dir):
                            continue
                        session_name = f"dev_{self._sanitize_name(repo_name)}_{self._sanitize_name(branch_dir)}"
                        has_active = session_name in active_sessions
                        last_commit = self._get_last_commit(wt_dir)
                        mtime = int(os.path.getmtime(wt_dir))
                        worktrees.append({
                            "repo_name": repo_name,
                            "branch": branch_dir,
                            "worktree_path": wt_dir,
                            "has_active_session": has_active,
                            "session_name": session_name,
                            "last_commit": last_commit,
                            "last_modified": mtime
                        })
                except Exception:
                    pass
            # 模式 2: 普通主仓库目录 (包含 .git)
            elif os.path.exists(os.path.join(entry_path, ".git")):
                repo_name = entry
                session_name = f"dev_{self._sanitize_name(repo_name)}"
                has_active = session_name in active_sessions
                last_commit = self._get_last_commit(entry_path)
                mtime = int(os.path.getmtime(entry_path))
                worktrees.append({
                    "repo_name": repo_name,
                    "branch": "main",
                    "worktree_path": entry_path,
                    "has_active_session": has_active,
                    "session_name": session_name,
                    "last_commit": last_commit,
                    "last_modified": mtime
                })
        return worktrees

    def session_exists(self, session_name: str) -> bool:
        return session_name in self.list_sessions()

    def _resolve_session_target(self, session_name: str) -> Optional[str]:
        """把已退出的 dev_<repo>[_<branch>] 会话名解析回其工作区或 Worktree 目录。"""
        if not session_name.startswith("dev_") or not os.path.isdir(self.workspace_root):
            return None

        for entry in sorted(os.listdir(self.workspace_root)):
            entry_path = os.path.join(self.workspace_root, entry)
            if not os.path.isdir(entry_path):
                continue
            if entry.endswith("-worktrees"):
                repo_name = entry[: -len("-worktrees")]
                try:
                    branch_dirs = sorted(os.listdir(entry_path))
                except OSError:
                    continue
                for branch_dir in branch_dirs:
                    wt_dir = os.path.join(entry_path, branch_dir)
                    if not os.path.isdir(wt_dir):
                        continue
                    candidate = f"dev_{self._sanitize_name(repo_name)}_{self._sanitize_name(branch_dir)}"
                    if candidate != session_name:
                        continue
                    registrations = list_worktree_registrations(wt_dir)
                    if registrations is None:
                        continue
                    if any(
                        os.path.abspath(item.get("path", "")) == os.path.abspath(wt_dir)
                        and self._sanitize_name(item.get("branch", ""))
                        == self._sanitize_name(branch_dir)
                        for item in registrations
                    ):
                        return wt_dir
            elif os.path.exists(os.path.join(entry_path, ".git")):
                if session_name == f"dev_{self._sanitize_name(entry)}":
                    return entry_path
        return None

    def _resolve_opencode_bin(self) -> str:
        """从当前 PATH 解析 opencode，并保留既有绝对路径作为兜底。"""
        resolved = shutil.which("opencode")
        if resolved:
            return resolved
        if os.path.exists(FALLBACK_OPENCODE_BIN):
            return FALLBACK_OPENCODE_BIN
        logger.warning("未在 PATH 或 %s 找到 opencode 可执行文件", FALLBACK_OPENCODE_BIN)
        return FALLBACK_OPENCODE_BIN

    def _build_init_command(self, project_dir: str, use_yolo: bool, extra_args: str) -> str:
        argv = [self._resolve_opencode_bin()]
        if use_yolo:
            argv.append("--yolo")
        if extra_args and extra_args.strip():
            argv.extend(shlex.split(extra_args))
        return shlex.join(argv)

    def _spawn_session(
        self,
        session_name: str,
        project_dir: str,
        use_yolo: bool,
        extra_args: str,
        attach: bool
    ) -> Dict[str, Any]:
        init_cmd = self._build_init_command(project_dir, use_yolo, extra_args)
        if attach:
            os.execvp("tmux", ["tmux", "new-session", "-s", session_name, "-c", project_dir, init_cmd])
            return {"success": True, "session_name": session_name, "is_new": True}

        res = subprocess.run(
            ["tmux", "new-session", "-d", "-s", session_name, "-c", project_dir, init_cmd],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False
        )
        if res.returncode != 0:
            raise RuntimeError(f"在后台创建 tmux 会话失败: {res.stderr.strip()}")
        return {
            "success": True,
            "session_name": session_name,
            "project_dir": project_dir,
            "is_new": True
        }

    def attach_session(self, session_name: Optional[str] = None):
        """直接 attach 到指定或唯一的活跃会话"""
        active = self.list_sessions()
        if not active:
            print("当前没有活跃的云端会话可供恢复。", file=sys.stderr)
            sys.exit(1)

        target = session_name
        if not target:
            if len(active) == 1:
                target = active[0]
            else:
                print("存在多个活跃会话，请指定要恢复的会话名称:", file=sys.stderr)
                for s in active:
                    print(f"  - {s}", file=sys.stderr)
                sys.exit(1)

        if not self.session_exists(target):
            if self.session_exists(f"dev_{target}"):
                target = f"dev_{target}"
            else:
                print(f"错误: 会话 [{target}] 不存在。", file=sys.stderr)
                sys.exit(1)

        print(f"正在恢复连接会话 [{target}]...")
        os.execvp("tmux", ["tmux", "attach-session", "-t", target])

    def _has_origin_remote(self, main_repo_dir: str) -> bool:
        res = subprocess.run(
            ["git", "-C", main_repo_dir, "remote"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False
        )
        if res.returncode != 0:
            return False
        return "origin" in res.stdout.split()

    def _fetch_origin(self, main_repo_dir: str) -> None:
        res = subprocess.run(
            ["git", "-C", main_repo_dir, "fetch", "--prune", "origin"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False
        )
        if res.returncode != 0:
            raise RuntimeError(f"拉取远程仓库 origin 失败，已停止创建 Git Worktree: {res.stderr.strip()}")

    def _resolve_new_branch_base(self, main_repo_dir: str) -> str:
        symref = subprocess.run(
            ["git", "-C", main_repo_dir, "symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False
        )
        if symref.returncode == 0 and symref.stdout.strip():
            return symref.stdout.strip()

        for candidate in ("origin/main", "origin/master"):
            probe = subprocess.run(
                ["git", "-C", main_repo_dir, "rev-parse", "--verify", "--quiet", candidate],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False
            )
            if probe.returncode == 0:
                return candidate

        return "HEAD"

    def _prepare_worktree(self, main_repo_dir: str, branch: str) -> str:
        """为指定的主仓库和分支创建或复用独立的 git worktree"""
        if not branch or branch.startswith("-"):
            raise RuntimeError(f"分支名无效，已拒绝执行 Git 操作: {branch!r}")

        ref_check = subprocess.run(
            ["git", "-C", main_repo_dir, "check-ref-format", "--branch", branch],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
        if ref_check.returncode != 0:
            raise RuntimeError(
                f"分支名未通过 Git 校验: {branch!r}: {(ref_check.stderr or '').strip()}"
            )

        repo_name = os.path.basename(main_repo_dir)
        clean_branch = self._sanitize_name(branch)
        worktree_base = os.path.join(self.workspace_root, f"{repo_name}-worktrees")
        os.makedirs(worktree_base, exist_ok=True)
        worktree_dir = os.path.join(worktree_base, clean_branch)

        registrations = list_worktree_registrations(main_repo_dir)
        if os.path.exists(worktree_dir):
            registered = (
                registrations is not None
                and any(
                    os.path.abspath(item.get("path", ""))
                    == os.path.abspath(worktree_dir)
                    and item.get("branch", "") == branch
                    for item in registrations
                )
            )
            if registered:
                return worktree_dir
            raise RuntimeError(
                f"目录 [{worktree_dir}] 已存在但未作为分支 [{branch}] 注册为 Git Worktree；"
                "已停止创建会话，请先人工确认该目录。"
            )

        print(f"正在为仓库 [{repo_name}] 创建隔离特性工作区 (分支: {branch})...")
        if self._has_origin_remote(main_repo_dir):
            self._fetch_origin(main_repo_dir)

        branch_check = subprocess.run(
            ["git", "-C", main_repo_dir, "rev-parse", "--verify", branch],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False
        )
        if branch_check.returncode == 0:
            cmd = ["git", "-C", main_repo_dir, "worktree", "add", worktree_dir, branch]
        else:
            base = self._resolve_new_branch_base(main_repo_dir)
            cmd = ["git", "-C", main_repo_dir, "worktree", "add", "-b", branch, worktree_dir, base]

        res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, check=False)
        if res.returncode != 0:
            raise RuntimeError(
                f"创建 Git Worktree 失败: {(res.stderr or '').strip()}"
            )

        return worktree_dir

    def start_or_attach(
        self,
        target: str,
        feature_branch: Optional[str] = None,
        use_yolo: bool = True,
        extra_args: str = "",
        attach: bool = True,
        restore_session: bool = True
    ) -> Dict[str, Any]:
        """
        进入目标项目并启动/恢复 tmux 会话中的 opencode。
        target: 可以是会话名、Git URL、本地相对/绝对路径或 workspace 下的目录名
        feature_branch: 可选的特性分支名。若提供，将自动基于目标仓库创建完全隔离的独立 Git Worktree 工作区。
        use_yolo: 是否完全授权 opencode (--yolo)
        extra_args: 传给 opencode 的额外自定义参数
        attach: 若为 True，通过 os.execvp 连入终端；若为 False，通过 tmux 后台创建并返回信息
        restore_session: 若为 True，dev_* 会话名在 tmux 进程退出后按原工作区重建；create 显式建仓时关闭
        """
        # 1. 若未指定 feature_branch，优先检查 target 是否是已存在的会话
        if not feature_branch:
            if self.session_exists(target):
                print(f"检测到已有活跃会话 [{target}]，正在恢复连接...")
                if attach:
                    os.execvp("tmux", ["tmux", "attach-session", "-t", target])
                return {"success": True, "session_name": target, "is_new": False}

            derived_session = f"dev_{self._sanitize_name(target)}"
            if self.session_exists(derived_session):
                print(f"检测到已有活跃会话 [{derived_session}]，正在恢复连接...")
                if attach:
                    os.execvp("tmux", ["tmux", "attach-session", "-t", derived_session])
                return {"success": True, "session_name": derived_session, "is_new": False}

            if restore_session and target.startswith("dev_"):
                project_dir = self._resolve_session_target(target)
                if project_dir is None:
                    raise RuntimeError(
                        f"会话 [{target}] 已不存在，且找不到与之对应的工作区或 Git Worktree；"
                        "为避免误建同名目录，请改用 cloud-session <repo> [branch] 创建。"
                    )
                print(f"会话 [{target}] 的 tmux 进程已退出，正在原目录 [{project_dir}] 重建同名会话...")
                return self._spawn_session(target, project_dir, use_yolo, extra_args, attach)

        # 2. 解析基础主仓库路径
        if target.startswith("http://") or target.startswith("https://") or target.startswith("git@"):
            repo_name = target.rstrip("/").split("/")[-1]
            if repo_name.endswith(".git"):
                repo_name = repo_name[:-4]
            main_repo_dir = os.path.join(self.workspace_root, repo_name)
            if not os.path.exists(main_repo_dir):
                print(f"正在克隆远程仓库 {target} 到 {main_repo_dir}...")
                subprocess.run(["git", "clone", target, main_repo_dir], check=True)
        else:
            ws_dir = os.path.join(self.workspace_root, target)
            if not os.path.exists(target) and os.path.exists(ws_dir):
                main_repo_dir = ws_dir
            elif not os.path.isabs(target) and not os.path.exists(target):
                main_repo_dir = os.path.join(self.workspace_root, target)
            else:
                main_repo_dir = os.path.abspath(target)
            repo_name = os.path.basename(main_repo_dir)

        # 3. 处理独立特性分支 Worktree 或主目录工作区
        if feature_branch:
            clean_branch = self._sanitize_name(feature_branch)
            session_name = f"dev_{self._sanitize_name(repo_name)}_{clean_branch}"

            # 若该特性的 tmux 会话已存在，直接连入
            if self.session_exists(session_name):
                print(f"检测到特性分支已有活跃会话 [{session_name}]，正在恢复连接...")
                if attach:
                    os.execvp("tmux", ["tmux", "attach-session", "-t", session_name])
                return {"success": True, "session_name": session_name, "is_new": False}

            project_dir = self._prepare_worktree(main_repo_dir, feature_branch)
        else:
            session_name = f"dev_{self._sanitize_name(repo_name)}"
            if self.session_exists(session_name):
                print(f"检测到已有会话 [{session_name}]，正在恢复连接...")
                if attach:
                    os.execvp("tmux", ["tmux", "attach-session", "-t", session_name])
                return {"success": True, "session_name": session_name, "is_new": False}

            if not os.path.exists(main_repo_dir):
                print(f"工作目录不存在，正在自动创建: {main_repo_dir}")
                os.makedirs(main_repo_dir, exist_ok=True)
            project_dir = main_repo_dir

        # 4. 创建全新 tmux 会话并在其中启动 opencode (按设置注入 --yolo 或自定义参数)
        yolo_desc = "已完全授权 (--yolo)" if use_yolo else "普通权限模式"
        print(f"创建全新长程编程会话 [{session_name}] (工作目录: {project_dir}, 授权模式: {yolo_desc})...")
        print("提示：合上 Mac 盖子或断开 SSH，该会话仍会在云端持续运行。")
        print(f"重新连入命令: cloud-session {session_name}\n")

        return self._spawn_session(session_name, project_dir, use_yolo, extra_args, attach)

    def _cleanup_after_kill(self, session_name: str, pane_paths: List[str]) -> None:
        panes_after = collect_tmux_panes()
        if panes_after is None:
            logger.warning(
                "无法检查 tmux 面板，保留会话 [%s] 关联的 Worktree", session_name
            )
            return
        active_paths = [
            pane.get("path", "")
            for pane in panes_after
            if pane.get("session") != session_name and pane.get("path")
        ]
        roots = {
            root
            for root in (
                find_worktree_root(path, self.workspace_root) for path in pane_paths
            )
            if root
        }
        for root in sorted(roots):
            try:
                registrations = list_worktree_registrations(root)
                cleanup_registered_worktree(
                    root,
                    self.workspace_root,
                    active_paths,
                    registrations=registrations,
                )
            except Exception as exc:
                logger.warning("会话终止后的 Worktree 清理失败 %s: %s", root, exc)

    def kill_session(self, session_name: str) -> bool:
        panes_before = collect_tmux_panes()
        pane_paths = (
            [
                pane.get("path", "")
                for pane in panes_before
                if pane.get("session") == session_name and pane.get("path")
            ]
            if panes_before is not None
            else []
        )
        res = subprocess.run(["tmux", "kill-session", "-t", session_name], check=False)
        if res.returncode != 0:
            return False
        try:
            self._cleanup_after_kill(session_name, pane_paths)
        except Exception as exc:
            logger.warning("会话终止后的清理失败 [%s]: %s", session_name, exc)
        return True


if __name__ == "__main__":
    runner = SessionRunner()
    args = sys.argv[1:]
    if not args:
        # 无参数时：若有 1 个活跃会话直接恢复；若有多个列出选择；若无活跃会话自动进入默认工作空间
        active = runner.list_sessions()
        if len(active) == 1:
            runner.attach_session(active[0])
            sys.exit(0)
        elif len(active) > 1:
            print("存在多个活跃会话，请选择恢复连接:", file=sys.stderr)
            for s in active:
                print(f"  - cloud-session {s}", file=sys.stderr)
            sys.exit(0)
        else:
            print("当前无活跃会话，正在默认工作区启动全新会话...")
            runner.start_or_attach("workspace")
            sys.exit(0)

    action = args[0]
    is_json = "--json" in args

    if action == "list":
        if is_json:
            print(json.dumps(runner.list_sessions_detailed(), ensure_ascii=False, indent=2))
        else:
            sessions = runner.list_sessions()
            if not sessions:
                print("当前没有活跃的云端会话。")
            else:
                print("活跃会话列表:")
                for s in sessions:
                    print(f"  - {s}")
    elif action == "worktrees":
        if is_json:
            print(json.dumps(runner.list_worktrees_detailed(), ensure_ascii=False, indent=2))
        else:
            worktrees = runner.list_worktrees_detailed()
            if not worktrees:
                print("当前没有已创建的 Git Worktree 工作区。")
            else:
                print("Git Worktree 工作区列表:")
                for wt in worktrees:
                    status_str = "[活跃运行中]" if wt["has_active_session"] else "[已离线/空闲]"
                    print(f"  - {wt['repo_name']} (分支: {wt['branch']}) {status_str} -> {wt['worktree_path']}")
    elif action == "attach":
        s_name = args[1] if len(args) > 1 and not args[1].startswith("-") else None
        runner.attach_session(s_name)
    elif action == "kill":
        if len(args) < 2:
            print("请指定要杀死的会话名称", file=sys.stderr)
            sys.exit(1)
        target_s = args[1]
        if runner.kill_session(target_s):
            if is_json:
                print(json.dumps({"success": True, "session_name": target_s}))
            else:
                print(f"会话 [{target_s}] 已成功终止。")
        else:
            if is_json:
                print(json.dumps({"success": False, "error": f"会话 [{target_s}] 终止失败或不存在"}))
                sys.exit(1)
            else:
                print(f"会话 [{target_s}] 终止失败或不存在。", file=sys.stderr)
                sys.exit(1)
    elif action == "create":
        # cloud-session create <repo> [branch] [--yolo | --no-yolo] [--extra-args <args>] [--no-attach]
        if len(args) < 2:
            print("用法: cloud-session create <repo> [branch] [--yolo|--no-yolo] [--no-attach]", file=sys.stderr)
            sys.exit(1)
        target_repo = args[1]
        target_branch = None
        idx = 2
        if idx < len(args) and not args[idx].startswith("-"):
            target_branch = args[idx]
            idx += 1
        
        yolo_opt = "--no-yolo" not in args
        no_attach = "--no-attach" in args or is_json
        extra_opt = ""
        if "--extra-args" in args:
            e_idx = args.index("--extra-args")
            if e_idx + 1 < len(args):
                extra_opt = args[e_idx + 1]

        try:
            res_info = runner.start_or_attach(
                target_repo,
                target_branch,
                use_yolo=yolo_opt,
                extra_args=extra_opt,
                attach=not no_attach,
                restore_session=False
            )
            if is_json:
                print(json.dumps(res_info, ensure_ascii=False, indent=2))
        except Exception as e:
            if is_json:
                print(json.dumps({"success": False, "error": str(e)}))
                sys.exit(1)
            else:
                print(f"创建会话失败: {e}", file=sys.stderr)
                sys.exit(1)
    else:
        # cloud-session <target> [branch] [--yolo|--no-yolo]
        s_name = None
        idx = 1
        if idx < len(args) and not args[idx].startswith("-"):
            s_name = args[idx]
            idx += 1
        yolo_opt = "--no-yolo" not in args
        extra_opt = ""
        if "--extra-args" in args:
            e_idx = args.index("--extra-args")
            if e_idx + 1 < len(args):
                extra_opt = args[e_idx + 1]

        try:
            runner.start_or_attach(action, s_name, use_yolo=yolo_opt, extra_args=extra_opt, attach=True)
        except RuntimeError as e:
            print(f"错误: {e}", file=sys.stderr)
            sys.exit(1)
