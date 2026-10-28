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

    def start_or_attach(self, target: str, session_name: Optional[str] = None):
        """
        进入目标项目并启动/恢复 tmux 会话中的 opencode。
        target: 可以是本地目录路径，也可以是 git clone URL (https://... 或 git@...)
        """
        # 1. 解析目标目录
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
            project_dir = os.path.abspath(target)
            if not os.path.exists(project_dir):
                print(f"错误: 本地目录不存在: {project_dir}", file=sys.stderr)
                sys.exit(1)
            if not session_name:
                base_name = os.path.basename(project_dir)
                session_name = f"dev_{self._sanitize_name(base_name)}"

        # 2. 检查会话是否存在
        if self.session_exists(session_name):
            print(f"检测到已有会话 [{session_name}]，正在恢复连接...")
            os.execvp("tmux", ["tmux", "attach-session", "-t", session_name])
            return

        # 3. 创建全新 tmux 会话并在其中启动 opencode
        print(f"创建全新长程编程会话 [{session_name}] (工作目录: {project_dir})...")
        print("提示：合上 Mac 盖子或断开 SSH，该会话仍会在云端持续运行。")
        print(f"重新连入命令: tmux attach -t {session_name}\n")

        init_cmd = f"cd {project_dir} && export PATH=$PATH:/root/.opencode/bin && opencode"
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
        print("用法:")
        print("  cloud-session <git-url或本地路径> [session_name]  # 启动或接入长程会话")
        print("  cloud-session list                             # 列出所有活跃会话")
        print("  cloud-session kill <session_name>              # 结束指定会话")
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
