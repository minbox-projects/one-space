#!/usr/bin/env python3
"""
onespace-cloud-agent: 磁盘空间守护者 (Disk Guardian)
针对 20G VPS 磁盘空间，提供任务临时工作目录释放、受控 OpenCode 缓存修剪与低磁盘告警。
"""

import logging
import os
import shutil
import subprocess
import time
from typing import Dict, List, Optional, Sequence

from core.session_runner import sweep_stale_worktrees as sweep_registered_worktrees

logger = logging.getLogger("onespace-guardian")

DEFAULT_DATA_ROOTS = (
    "~/.local/share/opencode",
    "~/.cache/opencode",
)
DEFAULT_CACHE_SUBDIRS = ("log", "tool-output")
LEGACY_SESSION_SUBDIR = "sessions"


class DiskGuardian:
    def __init__(
        self,
        opencode_dir: str = "/root/.opencode",
        min_free_gb: float = 2.0,
        data_roots: Optional[Sequence[str]] = None,
        cache_retention_days: int = 7,
        cleanup_sessions: bool = False,
        worktree_retention_days: int = 7,
        workspace_root: str = "/root/cloud-agent-workspaces",
    ):
        self.opencode_dir = opencode_dir
        self.min_free_gb = min_free_gb
        if data_roots is None:
            data_roots = DEFAULT_DATA_ROOTS
        self.data_roots: List[str] = [os.path.expanduser(root) for root in data_roots]
        self.cache_retention_days = cache_retention_days
        self.cleanup_sessions = cleanup_sessions
        self.worktree_retention_days = worktree_retention_days
        self.workspace_root = workspace_root

    def get_disk_usage(self, path: str = "/") -> Dict[str, float]:
        """获取指定路径的磁盘容量统计 (单位: GB)"""
        try:
            stat = os.statvfs(path)
            total_gb = (stat.f_blocks * stat.f_frsize) / (1024 ** 3)
            free_gb = (stat.f_bavail * stat.f_frsize) / (1024 ** 3)
            used_gb = total_gb - free_gb
            use_percent = (used_gb / total_gb) * 100 if total_gb > 0 else 0
            return {
                "total_gb": round(total_gb, 2),
                "used_gb": round(used_gb, 2),
                "free_gb": round(free_gb, 2),
                "use_percent": round(use_percent, 1)
            }
        except Exception as e:
            logger.error(f"获取磁盘使用情况失败: {e}")
            return {"total_gb": 0, "used_gb": 0, "free_gb": 0, "use_percent": 0}

    def cleanup_workspace(self, workspace_path: str):
        """即时清理一次任务执行产生的临时工作空间（任务自有 scratch，与共享 Worktree 分离）"""
        if not workspace_path or not os.path.exists(workspace_path):
            return

        logger.info(f"正在释放临时工作目录: {workspace_path}")
        try:
            if os.path.exists(os.path.join(workspace_path, ".git")):
                subprocess.run(["git", "-C", workspace_path, "clean", "-fdx"], check=False)
            shutil.rmtree(workspace_path, ignore_errors=True)
            logger.info(f"工作目录释放完毕: {workspace_path}")
        except Exception as e:
            logger.warning(f"清理工作目录失败: {e}")

    def _cleanup_allowlisted_tree(self, target_dir: str, cutoff_time: float) -> None:
        if os.path.islink(target_dir) or not os.path.isdir(target_dir):
            return

        empty_dirs: List[str] = []
        for root, dirs, files in os.walk(target_dir, topdown=True, followlinks=False):
            dirs[:] = [d for d in dirs if not os.path.islink(os.path.join(root, d))]
            for filename in files:
                file_path = os.path.join(root, filename)
                try:
                    if os.lstat(file_path).st_mtime >= cutoff_time:
                        continue
                    os.remove(file_path)
                    logger.info(f"已清理过期缓存条目: {file_path}")
                except OSError as exc:
                    logger.warning(f"清理缓存条目失败 {file_path}: {exc}")
            empty_dirs.append(root)

        for directory in sorted(empty_dirs, key=len, reverse=True):
            if os.path.abspath(directory) == os.path.abspath(target_dir):
                continue
            try:
                if not os.listdir(directory):
                    os.rmdir(directory)
            except OSError:
                pass

    def cleanup_cache(self) -> None:
        """仅清理允许目录中的过期缓存；默认保留全部会话文件。"""
        cutoff_time = time.time() - (self.cache_retention_days * 86400)
        subdirs = list(DEFAULT_CACHE_SUBDIRS)
        if self.cleanup_sessions:
            subdirs.append(LEGACY_SESSION_SUBDIR)

        for data_root in self.data_roots:
            if not os.path.isdir(data_root):
                continue
            for subdir in subdirs:
                self._cleanup_allowlisted_tree(
                    os.path.join(data_root, subdir), cutoff_time
                )

    def sweep_stale_worktrees(self) -> None:
        """通过共享原语清理过期、干净且无会话使用的已注册 Worktree。"""
        sweep_registered_worktrees(
            self.workspace_root, self.worktree_retention_days
        )

    def run_routine_check(self):
        """日常磁盘检查与防护逻辑"""
        usage = self.get_disk_usage("/")
        logger.info(f"磁盘巡检: 剩余 {usage['free_gb']} GB / 总计 {usage['total_gb']} GB ({usage['use_percent']}%)")
        if usage["free_gb"] < self.min_free_gb:
            logger.warning(
                f"磁盘剩余空间已低于 {self.min_free_gb} GB 阈值，执行受控缓存清理..."
            )
        self.cleanup_cache()
        self.sweep_stale_worktrees()


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)
    guardian = DiskGuardian()
    print("磁盘状态:", guardian.get_disk_usage())
    guardian.cleanup_cache()
    guardian.sweep_stale_worktrees()
