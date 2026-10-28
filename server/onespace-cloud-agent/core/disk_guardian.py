#!/usr/bin/env python3
"""
onespace-cloud-agent: 磁盘空间守护者 (Disk Guardian)
针对 20G VPS 磁盘空间，提供工作目录即时释放、OpenCode 历史会话自动修剪与低磁盘告警。
"""

import logging
import os
import shutil
import subprocess
import time
from typing import Dict

logger = logging.getLogger("onespace-guardian")


class DiskGuardian:
    def __init__(self, opencode_dir: str = "/root/.opencode", min_free_gb: float = 2.0):
        self.opencode_dir = opencode_dir
        self.min_free_gb = min_free_gb

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
        """即时清理一次任务执行产生的临时工作空间"""
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

    def cleanup_opencode_history(self, max_age_days: int = 3):
        """修剪超过保留期的 OpenCode 会话日志与缓存"""
        if not os.path.exists(self.opencode_dir):
            return

        cutoff_time = time.time() - (max_age_days * 86400)
        reclaimed_bytes = 0
        deleted_count = 0

        scan_subdirs = ["generated-images", "cache", "logs", "sessions"]
        for sub in scan_subdirs:
            target_dir = os.path.join(self.opencode_dir, sub)
            if not os.path.exists(target_dir):
                continue
            for root, dirs, files in os.walk(target_dir):
                for f in files:
                    file_path = os.path.join(root, f)
                    try:
                        mtime = os.path.getmtime(file_path)
                        if mtime < cutoff_time:
                            size = os.path.getsize(file_path)
                            os.remove(file_path)
                            reclaimed_bytes += size
                            deleted_count += 1
                    except Exception:
                        pass

        for sub in scan_subdirs:
            target_dir = os.path.join(self.opencode_dir, sub)
            if os.path.exists(target_dir):
                for root, dirs, files in os.walk(target_dir, topdown=False):
                    for d in dirs:
                        dir_path = os.path.join(root, d)
                        try:
                            if not os.listdir(dir_path):
                                os.rmdir(dir_path)
                        except Exception:
                            pass

        mb_freed = round(reclaimed_bytes / (1024 * 1024), 2)
        if deleted_count > 0:
            logger.info(f"OpenCode 历史修剪完成: 清理 {deleted_count} 个过期文件，释放 {mb_freed} MB")

    def run_routine_check(self):
        """日常磁盘检查与防护逻辑"""
        usage = self.get_disk_usage("/")
        logger.info(f"磁盘巡检: 剩余 {usage['free_gb']} GB / 总计 {usage['total_gb']} GB ({usage['use_percent']}%)")
        if usage["free_gb"] < self.min_free_gb:
            logger.warning(f"磁盘剩余空间已低于 {self.min_free_gb} GB 阈值，执行深度自动修剪...")
            self.cleanup_opencode_history(max_age_days=1)
            subprocess.run(["apt-get", "clean"], check=False)
            subprocess.run(["journalctl", "--vacuum-size=50M"], check=False)


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)
    guardian = DiskGuardian()
    print("磁盘状态:", guardian.get_disk_usage())
    guardian.cleanup_opencode_history(max_age_days=3)
