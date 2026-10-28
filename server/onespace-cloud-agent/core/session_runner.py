#!/usr/bin/env python3
"""
onespace-cloud-agent: 长程编程与重构会话管理器 (Session Runner)
基于 tmux 实现断网保活、Mac 合盖即走、随时开盖恢复现场。
"""

import os
import re
import subprocess
import sys
from typing import List, Optional


class SessionRunner:
    def __init__(self, workspace_root: str = "/root/cloud-agent-workspaces"):
        self.workspace_root = workspace_root
        os.makedirs(self.workspace_root, exist_ok=True)

    def _sanitize_name(self, name: str) -> str:
        return re.sub(r"[^a-zA-Z0-9_\-]", "_", name)

    def list_sessions(self) -> List[str]:
        """列出所有活跃的 tmux 会话"""
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

    def session_exists(self, session_name: str) -> bool:
        return session_name in self.list_sessions()

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
        branch_check = subprocess.run(
            ["git", "-C", main_repo_dir, "rev-parse", "--verify", branch],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False
        )
        if branch_check.returncode == 0:
            cmd = ["git", "-C", main_repo_dir, "worktree", "add", worktree_dir, branch]
        else:
            cmd = ["git", "-C", main_repo_dir, "worktree", "add", "-b", branch, worktree_dir]

        res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, check=False)
        if res.returncode != 0 and not os.path.exists(worktree_dir):
            raise RuntimeError(f"创建 Git Worktree 失败: {res.stderr.strip()}")

        return worktree_dir

    def start_or_attach(self, target: str, feature_branch: Optional[str] = None):
        """
        进入目标项目并启动/恢复 tmux 会话中的 opencode。
        target: 可以是会话名、Git URL、本地相对/绝对路径或 workspace 下的目录名
        feature_branch: 可选的特性分支名。若提供，将自动基于目标仓库创建完全隔离的独立 Git Worktree 工作区。
        """
        # 1. 若未指定 feature_branch，优先检查 target 是否是已存在的会话
        if not feature_branch:
            if self.session_exists(target):
                print(f"检测到已有活跃会话 [{target}]，正在恢复连接...")
                os.execvp("tmux", ["tmux", "attach-session", "-t", target])
                return

            derived_session = f"dev_{self._sanitize_name(target)}"
            if self.session_exists(derived_session):
                print(f"检测到已有活跃会话 [{derived_session}]，正在恢复连接...")
                os.execvp("tmux", ["tmux", "attach-session", "-t", derived_session])
                return

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
                os.execvp("tmux", ["tmux", "attach-session", "-t", session_name])
                return

            project_dir = self._prepare_worktree(main_repo_dir, feature_branch)
        else:
            session_name = f"dev_{self._sanitize_name(repo_name)}"
            if self.session_exists(session_name):
                print(f"检测到已有会话 [{session_name}]，正在恢复连接...")
                os.execvp("tmux", ["tmux", "attach-session", "-t", session_name])
                return

            if not os.path.exists(main_repo_dir):
                print(f"工作目录不存在，正在自动创建: {main_repo_dir}")
                os.makedirs(main_repo_dir, exist_ok=True)
            project_dir = main_repo_dir

        # 4. 创建全新 tmux 会话并在其中启动 opencode (默认开启 --yolo 全权限自动授权模式)
        print(f"创建全新长程编程会话 [{session_name}] (工作目录: {project_dir}, 默认授权全部权限 --yolo)...")
        print("提示：合上 Mac 盖子或断开 SSH，该会话仍会在云端持续运行。")
        print(f"重新连入命令: cloud-session {session_name}\n")

        init_cmd = f"cd {project_dir} && export PATH=$PATH:/root/.opencode/bin && opencode --yolo"
        cmd = [
            "tmux", "new-session",
            "-s", session_name,
            "-c", project_dir,
            init_cmd
        ]
        os.execvp("tmux", cmd)

    def kill_session(self, session_name: str) -> bool:
        res = subprocess.run(["tmux", "kill-session", "-t", session_name], check=False)
        return res.returncode == 0


if __name__ == "__main__":
    runner = SessionRunner()
    if len(sys.argv) < 2:
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

    action = sys.argv[1]
    if action == "list":
        sessions = runner.list_sessions()
        if not sessions:
            print("当前没有活跃的云端会话。")
        else:
            print("活跃会话列表:")
            for s in sessions:
                print(f"  - {s}")
    elif action == "attach":
        s_name = sys.argv[2] if len(sys.argv) > 2 else None
        runner.attach_session(s_name)
    elif action == "kill":
        if len(sys.argv) < 3:
            print("请指定要杀死的会话名称", file=sys.stderr)
            sys.exit(1)
        target_s = sys.argv[2]
        if runner.kill_session(target_s):
            print(f"会话 [{target_s}] 已成功终止。")
        else:
            print(f"会话 [{target_s}] 终止失败或不存在。", file=sys.stderr)
    else:
        s_name = sys.argv[2] if len(sys.argv) > 2 else None
        runner.start_or_attach(action, s_name)
