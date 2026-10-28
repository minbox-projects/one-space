#!/usr/bin/env python3
"""
onespace-cloud-agent: 长程编程与重构会话管理器 (Session Runner)
基于 tmux 实现断网保活、Mac 合盖即走、随时开盖恢复现场。
"""

import json
import os
import re
import shlex
import subprocess
import sys
import time
from typing import Any, Dict, List, Optional


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

                results.append({
                    "session_name": session_name,
                    "repo_name": repo_name,
                    "branch": branch,
                    "working_dir": working_dir,
                    "created_at": created_at,
                    "windows_count": windows_count,
                    "pane_pid": pane_pid,
                    "is_active": True
                })
            return results
        except Exception:
            return []

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
                    if candidate == session_name:
                        return wt_dir
            elif os.path.exists(os.path.join(entry_path, ".git")):
                if session_name == f"dev_{self._sanitize_name(entry)}":
                    return entry_path
        return None

    def _build_init_command(self, project_dir: str, use_yolo: bool, extra_args: str) -> str:
        auth_flag = "--yolo" if use_yolo else ""
        extra_flag = f" {extra_args.strip()}" if extra_args and extra_args.strip() else ""
        return (
            f"cd {shlex.quote(project_dir)} && export PATH=$PATH:/root/.opencode/bin && "
            f"opencode {auth_flag}{extra_flag}; "
            f"echo 'opencode 已退出，当前为兜底 shell；输入 oc 重新进入，或 exit 结束会话'; "
            f"exec bash -l"
        ).strip()

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
        repo_name = os.path.basename(main_repo_dir)
        clean_branch = self._sanitize_name(branch)
        worktree_base = os.path.join(self.workspace_root, f"{repo_name}-worktrees")
        os.makedirs(worktree_base, exist_ok=True)
        worktree_dir = os.path.join(worktree_base, clean_branch)

        if os.path.exists(worktree_dir):
            return worktree_dir

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
        if res.returncode != 0 and not os.path.exists(worktree_dir):
            raise RuntimeError(f"创建 Git Worktree 失败: {res.stderr.strip()}")

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

    def kill_session(self, session_name: str) -> bool:
        res = subprocess.run(["tmux", "kill-session", "-t", session_name], check=False)
        return res.returncode == 0


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
