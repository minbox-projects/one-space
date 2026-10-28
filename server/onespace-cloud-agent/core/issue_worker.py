#!/usr/bin/env python3
"""
onespace-cloud-agent: GitHub Issue 监听与自动 PR 流水线核心组件
实现多仓库监听、自动接单、无头调用 opencode run 实施需求、提交 PR 并闭环通知。
"""

import json
import logging
import os
import subprocess
import time
from typing import Any, Dict, List, Optional
from core.disk_guardian import DiskGuardian
from core.notifier import Notifier

logger = logging.getLogger("onespace-worker")


class IssueWorker:
    def __init__(self, config: Dict[str, Any], guardian: DiskGuardian, notifier: Notifier):
        worker_cfg = config.get("issue_worker", {})
        self.enabled: bool = worker_cfg.get("enabled", True)
        self.github_token: str = worker_cfg.get("github_token", "")
        if self.github_token:
            os.environ["GH_TOKEN"] = self.github_token
            os.environ["GITHUB_TOKEN"] = self.github_token
            logger.info("已从配置文件加载 GitHub Token (GH_TOKEN)")

        self.watch_repositories: List[str] = worker_cfg.get("watch_repositories", [])
        self.trigger_label: str = worker_cfg.get("trigger_label", "bot:task")
        self.poll_interval: int = worker_cfg.get("poll_interval_seconds", 180)
        self.workspace_root: str = worker_cfg.get("workspace_root", "/root/cloud-agent-workspaces")
        self.state_file: str = os.path.expanduser("~/.config/onespace-cloud-agent/worker_state.json")
        self.guardian = guardian
        self.notifier = notifier
        self.processed_issues: Dict[str, List[int]] = {}  # {repo: [issue_numbers]}
        self.load_state()

    def load_state(self):
        os.makedirs(os.path.dirname(self.state_file), exist_ok=True)
        if os.path.exists(self.state_file):
            try:
                with open(self.state_file, "r", encoding="utf-8") as f:
                    self.processed_issues = json.load(f)
            except Exception as e:
                logger.warning(f"读取任务状态文件失败: {e}")
                self.processed_issues = {}

    def save_state(self):
        try:
            with open(self.state_file, "w", encoding="utf-8") as f:
                json.dump(self.processed_issues, f, ensure_ascii=False, indent=2)
        except Exception as e:
            logger.warning(f"保存任务状态文件失败: {e}")

    def is_issue_processed(self, repo: str, issue_number: int) -> bool:
        return issue_number in self.processed_issues.get(repo, [])

    def mark_issue_processed(self, repo: str, issue_number: int):
        if repo not in self.processed_issues:
            self.processed_issues[repo] = []
        if issue_number not in self.processed_issues[repo]:
            self.processed_issues[repo].append(issue_number)
        self.save_state()

    def scan_repo_issues(self, repo: str) -> List[Dict[str, Any]]:
        """扫描单个仓库符合条件的待处理 Issue"""
        try:
            cmd = [
                "gh", "issue", "list",
                "--repo", repo,
                "--label", self.trigger_label,
                "--state", "open",
                "--json", "number,title,body,labels,author",
                "--limit", "10"
            ]
            res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, check=False)
            if res.returncode != 0:
                logger.warning(f"[{repo}] 扫描 Issue 失败: {res.stderr.strip()}")
                return []
            raw = res.stdout.strip()
            return json.loads(raw) if raw else []
        except Exception as e:
            logger.error(f"[{repo}] 扫描过程发生异常: {e}")
            return []

    def comment_issue(self, repo: str, issue_number: int, comment: str):
        """向指定 Issue 发表回复"""
        try:
            subprocess.run(
                ["gh", "issue", "comment", str(issue_number), "--repo", repo, "--body", comment],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False
            )
        except Exception as e:
            logger.warning(f"[{repo}#{issue_number}] 回复 Issue 失败: {e}")

    def execute_issue_task(self, repo: str, issue: Dict[str, Any]):
        issue_number = issue.get("number")
        issue_title = issue.get("title", "")
        issue_body = issue.get("body", "")

        logger.info(f"========== 开始处理任务: [{repo}] #{issue_number} {issue_title} ==========")
        task_dir = None

        try:
            # 1. 发送接单通知与 Issue 评论
            self.comment_issue(
                repo, issue_number,
                f"🤖 **OneSpace Cloud Agent 已接单**\n\n正在云端独立环境中分析需求与代码库，开始实施功能，请稍候..."
            )
            self.notifier.notify_task_started(repo, issue_number, issue_title)

            # 2. 准备隔离的工作空间与分支
            clean_repo_name = repo.replace("/", "_")
            timestamp = int(time.time())
            task_dir = os.path.join(self.workspace_root, f"task_{clean_repo_name}_{issue_number}_{timestamp}")
            os.makedirs(task_dir, exist_ok=True)

            logger.info(f"克隆仓库到工作目录: {task_dir}")
            clone_cmd = ["gh", "repo", "clone", repo, task_dir]
            clone_res = subprocess.run(clone_cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, check=False)
            if clone_res.returncode != 0:
                raise RuntimeError(f"克隆仓库失败: {clone_res.stderr.strip()}")

            # 配置本地 git 提交者
            subprocess.run(["git", "-C", task_dir, "config", "user.name", "OneSpace Cloud Bot"], check=False)
            subprocess.run(["git", "-C", task_dir, "config", "user.email", "bot@onespace.local"], check=False)

            branch_name = f"bot/issue-{issue_number}-{timestamp}"
            logger.info(f"创建分支: {branch_name}")
            subprocess.run(["git", "-C", task_dir, "checkout", "-b", branch_name], check=True)

            # 3. 组织 Prompt 并调用 opencode run
            prompt = (
                f"请根据以下 GitHub Issue 需求，分析代码并完成修改实现：\n\n"
                f"【Issue 标题】: {issue_title}\n"
                f"【需求描述】:\n{issue_body}\n\n"
                f"【实施约束与要求】:\n"
                f"1. 严格遵守项目现有的代码规范、架构风格与技术栈约定。\n"
                f"2. 仅做必要的代码与用例变更，保持简洁与高可维护性（KISS 原则）。\n"
                f"3. 实施完成后，请检查语法与运行轻量级单元测试确保改动无破坏性。"
            )

            logger.info("正在调用 opencode run 无头批量执行需求实现...")
            env = os.environ.copy()
            env["PATH"] = f"/root/.opencode/bin:{env.get('PATH', '')}"

            run_res = subprocess.run(
                ["opencode", "run", "--yolo", prompt],
                cwd=task_dir,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                timeout=600,
                check=False
            )

            logger.info(f"opencode 执行输出摘要: {run_res.stdout[:300]}")
            if run_res.returncode != 0:
                logger.warning(f"opencode 执行返回非零状态: {run_res.stderr[:300]}")

            # 4. 检查是否有有效 git diff
            diff_res = subprocess.run(["git", "-C", task_dir, "status", "--porcelain"], stdout=subprocess.PIPE, text=True)
            if not diff_res.stdout.strip():
                logger.info("代码库无实际变更产生")
                self.comment_issue(
                    repo, issue_number,
                    "⚠️ **执行反馈**: OpenCode 分析与执行完成，但未产生任何代码修改。请检查 Issue 描述是否清晰或已实现。"
                )
                self.mark_issue_processed(repo, issue_number)
                return

            # 5. 提交代码并推送到远端分支
            logger.info("代码产生有效变更，正在提交与推送到远端...")
            subprocess.run(["git", "-C", task_dir, "add", "-A"], check=True)
            commit_msg = f"feat(issue-#{issue_number}): {issue_title}"
            subprocess.run(["git", "-C", task_dir, "commit", "-m", commit_msg], check=True)
            subprocess.run(["git", "-C", task_dir, "push", "-u", "origin", branch_name], check=True)

            # 6. 创建 Pull Request
            pr_title = f"feat: resolve #{issue_number} ({issue_title})"
            pr_body = (
                f"Closes #{issue_number}\n\n"
                f"### 🤖 OneSpace Cloud Agent 实施说明\n\n"
                f"根据 Issue #{issue_number} 由云端自主编程 Agent 完成功能修改与用例补充。\n\n"
                f"请 Review 代码并合并。"
            )

            logger.info("正在创建 GitHub Pull Request...")
            pr_cmd = [
                "gh", "pr", "create",
                "--repo", repo,
                "--title", pr_title,
                "--body", pr_body,
                "--head", branch_name
            ]
            pr_res = subprocess.run(pr_cmd, cwd=task_dir, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, check=False)
            if pr_res.returncode != 0:
                raise RuntimeError(f"创建 PR 失败: {pr_res.stderr.strip()}")

            pr_url = pr_res.stdout.strip()
            logger.info(f"Pull Request 创建成功: {pr_url}")

            # 7. Issue 闭环回复与通知
            completion_comment = (
                f"🎉 **需求已实施完毕！**\n\n"
                f"- **关联分支**: `{branch_name}`\n"
                f"- **Pull Request**: [点击查看代码改动并评审]({pr_url})\n\n"
                f"请核对功能并进行代码合并。"
            )
            self.comment_issue(repo, issue_number, completion_comment)
            self.notifier.notify_task_completed(repo, issue_number, issue_title, pr_url)
            self.mark_issue_processed(repo, issue_number)

        except Exception as e:
            error_msg = str(e)
            logger.error(f"任务实施过程发生异常: {error_msg}")
            self.comment_issue(
                repo, issue_number,
                f"❌ **任务执行异常**\n\n执行过程中遇到错误: ```{error_msg}```\n请检查云端日志或介入排查。"
            )
            self.notifier.notify_task_failed(repo, issue_number, issue_title, error_msg)
            self.mark_issue_processed(repo, issue_number)

        finally:
            # 8. 磁盘空间即时释放
            if task_dir and os.path.exists(task_dir):
                self.guardian.cleanup_workspace(task_dir)

    def run_poll_cycle(self):
        """执行一次完整的轮询循环"""
        if not self.enabled or not self.watch_repositories:
            return

        for repo in self.watch_repositories:
            repo = repo.strip()
            if not repo:
                continue
            logger.info(f"正在扫描仓库 [{repo}] 待处理任务 (Label: {self.trigger_label})...")
            issues = self.scan_repo_issues(repo)
            for issue in issues:
                number = issue.get("number")
                if not self.is_issue_processed(repo, number):
                    self.execute_issue_task(repo, issue)
                    self.guardian.run_routine_check()


def start_worker_loop(worker: IssueWorker):
    logger.info("OneSpace Issue Worker 扫描循环启动...")
    while True:
        try:
            worker.run_poll_cycle()
        except Exception as e:
            logger.error(f"扫描循环发生未捕获异常: {e}")
        time.sleep(max(10, worker.poll_interval))
