#!/usr/bin/env python3
"""
OneSpace Clean Script
清空 OneSpace 所有数据和配置目录
"""

import os
import shutil
import argparse
from pathlib import Path
from typing import List, Tuple


DIRECTORIES_TO_CLEAN = [
    "~/.config/onespace",
    "~/.codex/skills",
    "~/.codex/agents",
    "~/.claude/skills",
    "~/.claude/agents",
    "~/.gemini/skills",
    "~/.gemini/agents",
    "~/.config/opencode/skills",
    "~/.config/opencode/agents",
]


def expand_path(path: str) -> Path:
    """展开 ~ 为完整路径"""
    return Path(path).expanduser()


def check_directories() -> List[Tuple[Path, bool, int]]:
    """
    检查所有待清空目录
    返回：[(路径，是否存在，大小字节)]
    """
    results = []
    for dir_path in DIRECTORIES_TO_CLEAN:
        path = expand_path(dir_path)
        if path.exists():
            size = get_directory_size(path)
            results.append((path, True, size))
        else:
            results.append((path, False, 0))
    return results


def get_directory_size(path: Path) -> int:
    """计算目录总大小（字节）"""
    total_size = 0
    try:
        for item in path.rglob("*"):
            if item.is_file():
                try:
                    total_size += item.stat().st_size
                except (OSError, PermissionError):
                    pass
    except (OSError, PermissionError):
        pass
    return total_size


def format_size(size_bytes: int) -> str:
    """格式化文件大小显示"""
    size = float(size_bytes)
    for unit in ["B", "KB", "MB", "GB"]:
        if size < 1024.0:
            return f"{size:.2f} {unit}"
        size /= 1024.0
    return f"{size:.2f} TB"


def check_mode():
    """检查模式：列出待删除的目录"""
    print("=" * 60)
    print("OneSpace Clean - 检查模式")
    print("=" * 60)
    print()

    results = check_directories()

    existing_dirs = [(path, size) for path, exists, size in results if exists]
    non_existing_dirs = [path for path, exists, _ in results if not exists]

    if existing_dirs:
        print("以下目录将被清空:\n")
        total_size = 0
        for path, size in existing_dirs:
            print(f"  {path}")
            print(f"    大小：{format_size(size)}")
            total_size += size
        print()
        print(f"总计：{len(existing_dirs)} 个目录，{format_size(total_size)}")
    else:
        print("没有找到需要清空的目录")

    if non_existing_dirs:
        print(f"\n以下目录不存在:\n")
        for path in non_existing_dirs:
            print(f"  {path}")

    print()
    print("=" * 60)

    if existing_dirs:
        print("\n执行清空命令：python3 skills/onespace-clean/scripts/clean.py --yes")
    print()

    return len(existing_dirs)


def clean_directories(
    dry_run: bool = False,
) -> Tuple[List[Path], List[Tuple[Path, str]]]:
    """
    执行清空操作
    返回：(成功列表，失败列表 [(路径，错误信息)])
    """
    results = check_directories()
    success = []
    failed = []

    for path, exists, size in results:
        if not exists:
            continue

        if dry_run:
            print(f"[DRY RUN] 将删除：{path} ({format_size(size)})")
            success.append(path)
            continue

        try:
            shutil.rmtree(path)
            success.append(path)
            print(f"[已删除] {path}")
        except PermissionError as e:
            failed.append((path, f"权限错误：{e}"))
            print(f"[失败] {path} - 权限错误")
        except OSError as e:
            failed.append((path, f"系统错误：{e}"))
            print(f"[失败] {path} - {e}")
        except Exception as e:
            failed.append((path, f"未知错误：{e}"))
            print(f"[失败] {path} - {e}")

    return success, failed


def confirm_prompt() -> bool:
    """显示确认提示"""
    print("\n⚠️  警告：此操作将永久删除所有配置和数据！")
    response = input("确认清空？(yes/no): ")
    return response.lower() in ["yes", "y"]


def main():
    parser = argparse.ArgumentParser(description="清空 OneSpace 所有数据和配置")
    parser.add_argument(
        "--check", action="store_true", help="检查模式：仅列出待删除的目录"
    )
    parser.add_argument("--yes", action="store_true", help="自动确认执行清空操作")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="模拟运行：显示将要删除的内容但不实际删除",
    )

    args = parser.parse_args()

    if args.check or not (args.yes or args.dry_run):
        check_mode()
        return

    if args.dry_run:
        print("=" * 60)
        print("OneSpace Clean - 模拟运行")
        print("=" * 60)
        print()
        clean_directories(dry_run=True)
        print()
        print("模拟运行完成，未删除任何文件")
        return

    if not args.yes:
        if not confirm_prompt():
            print("操作已取消")
            return

    print("=" * 60)
    print("OneSpace Clean - 执行清空")
    print("=" * 60)
    print()

    success, failed = clean_directories()

    print()
    print("=" * 60)
    print(f"清空完成：{len(success)} 个成功，{len(failed)} 个失败")

    if failed:
        print("\n失败详情:")
        for path, error in failed:
            print(f"  {path}: {error}")

    print()


if __name__ == "__main__":
    main()
