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

    def start_or_attach(self, target: str, session_name: Optional[str] = None):
        """
        进入目标项目并启动/恢复 tmux 会话中的 opencode。
        target: 可以是会话名、Git URL、本地相对/绝对路径或 workspace 下的目录名
        """
        # 1. 优先检查 target 本身是否就是一个已存在的 tmux 会话
        if self.session_exists(target):
            print(f"检测到已有活跃会话 [{target}]，正在恢复连接...")
            os.execvp("tmux", ["tmux", "attach-session", "-t", target])
            return

        # 2. 检查 dev_<target> 是否是已存在的会话
        derived_session = f"dev_{self._sanitize_name(target)}"
        if self.session_exists(derived_session):
            print(f"检测到已有活跃会话 [{derived_session}]，正在恢复连接...")
            os.execvp("tmux", ["tmux", "attach-session", "-t", derived_session])
            return

        # 3. 检查显式传入的 session_name 是否已存在
        if session_name and self.session_exists(session_name):
            print(f"检测到已有活跃会话 [{session_name}]，正在恢复连接...")
            os.execvp("tmux", ["tmux", "attach-session", "-t", session_name])
            return

        # 4. 检查是否在 workspace_root 中存在该子目录（例如传入 "one-space"）
        ws_dir = os.path.join(self.workspace_root, target)
        if not os.path.exists(target) and os.path.exists(ws_dir):
            target = ws_dir

        # 5. 解析 target 是远程 Git 仓库还是本地目录
        if target.startswith("http://") or target.startswith("https://") or target.startswith("git@"):
            repo_name = target.rstrip("/").split("/")[-1]
            if repo_name.endswith(".git"):
                repo_name = repo_name[:-4]
            project_dir = os.path.join(self.workspace_root, repo_name)
            if not os.path.exists(project_dir):
                print(f"正在克隆远程仓库 {target} 到 {project_dir}...")
                subprocess.run(["git", "clone", target, project_dir], check=True)
            if not session_name:
                session_name = f"dev_{self._sanitize_name(repo_name)}"
        else:
            if not os.path.isabs(target) and not os.path.exists(target):
                project_dir = os.path.join(self.workspace_root, target)
            else:
                project_dir = os.path.abspath(target)

            if not os.path.exists(project_dir):
                print(f"工作目录不存在，正在自动创建: {project_dir}")
                os.makedirs(project_dir, exist_ok=True)

            if not session_name:
                base_name = os.path.basename(project_dir)
                session_name = f"dev_{self._sanitize_name(base_name)}"

        # 6. 如果生成的 session_name 已经存在，恢复它
        if self.session_exists(session_name):
            print(f"检测到已有会话 [{session_name}]，正在恢复连接...")
            os.execvp("tmux", ["tmux", "attach-session", "-t", session_name])
            return

        # 7. 创建全新 tmux 会话并在其中启动 opencode (默认开启 --yolo 全权限自动授权模式)
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
