#!/usr/bin/env python3
"""
onespace-cloud-agent: 多渠道通知推送模块 (Notifier)
支持钉钉自定义机器人（含安全加签）与企业微信群机器人。
"""

import base64
import hashlib
import hmac
import json
import logging
import time
import urllib.parse
import urllib.request
from typing import Any, Dict, Optional

logger = logging.getLogger("onespace-notifier")


class Notifier:
    def __init__(self, config: Dict[str, Any]):
        notify_cfg = config.get("notification", {})
        self.enabled: bool = notify_cfg.get("enabled", False)
        self.dingtalk_url: Optional[str] = notify_cfg.get("dingtalk_webhook")
        self.dingtalk_secret: Optional[str] = notify_cfg.get("dingtalk_secret")
        self.wechat_url: Optional[str] = notify_cfg.get("wechat_work_webhook")

    def _send_http_json(self, url: str, payload: Dict[str, Any]) -> bool:
        try:
            data = json.dumps(payload, ensure_ascii=False).encode("utf-8")
            req = urllib.request.Request(
                url,
                data=data,
                headers={"Content-Type": "application/json; charset=utf-8"},
                method="POST"
            )
            with urllib.request.urlopen(req, timeout=10) as resp:
                result = resp.read().decode("utf-8")
                logger.info(f"通知推送响应: {result[:120]}")
                return resp.status == 200
        except Exception as e:
            logger.error(f"通知推送失败 [{url}]: {e}")
            return False

    def build_dingtalk_signed_url(self, timestamp: Optional[str] = None) -> Optional[str]:
        if not self.dingtalk_url:
            return None
        if not self.dingtalk_secret:
            return self.dingtalk_url

        if not timestamp:
            timestamp = str(round(time.time() * 1000))
        secret_enc = self.dingtalk_secret.encode("utf-8")
        string_to_sign = f"{timestamp}\n{self.dingtalk_secret}"
        string_to_sign_enc = string_to_sign.encode("utf-8")
        hmac_code = hmac.new(secret_enc, string_to_sign_enc, digestmod=hashlib.sha256).digest()
        sign = urllib.parse.quote_plus(base64.b64encode(hmac_code))
        
        sep = "&" if "?" in self.dingtalk_url else "?"
        return f"{self.dingtalk_url}{sep}timestamp={timestamp}&sign={sign}"

    def send_dingtalk(self, title: str, markdown_content: str):
        target_url = self.build_dingtalk_signed_url()
        if not target_url:
            return
        payload = {
            "msgtype": "markdown",
            "markdown": {
                "title": title,
                "text": f"### {title}\n\n{markdown_content}\n\n> 🤖 来自 OneSpace Cloud Agent"
            }
        }
        self._send_http_json(target_url, payload)

    def send_wechat(self, title: str, markdown_content: str):
        if not self.wechat_url:
            return
        payload = {
            "msgtype": "markdown",
            "markdown": {
                "content": f"### {title}\n{markdown_content}\n> 🤖 来自 OneSpace Cloud Agent"
            }
        }
        self._send_http_json(self.wechat_url, payload)

    def notify(self, title: str, markdown_content: str):
        if not self.enabled:
            return
        if self.dingtalk_url:
            self.send_dingtalk(title, markdown_content)
        if self.wechat_url:
            self.send_wechat(title, markdown_content)

    def notify_task_started(self, repo: str, issue_number: int, issue_title: str):
        title = f"🚀 任务接单: #{issue_number} ({repo})"
        content = (
            f"- **目标仓库**: `{repo}`\n"
            f"- **Issue 编号**: `#{issue_number}`\n"
            f"- **需求标题**: **{issue_title}**\n"
            f"- **当前状态**: 云端 Agent 正在隔离环境中分析与实施..."
        )
        self.notify(title, content)

    def notify_task_completed(self, repo: str, issue_number: int, issue_title: str, pr_url: str):
        title = f"🎉 需求完成并生成 PR: #{issue_number} ({repo})"
        content = (
            f"- **目标仓库**: `{repo}`\n"
            f"- **关联 Issue**: `#{issue_number} - {issue_title}`\n"
            f"- **已生成 PR**: [点击查看 Pull Request]({pr_url})\n"
            f"- **后续步骤**: 请前往 GitHub 进行代码审查与合并"
        )
        self.notify(title, content)

    def notify_task_failed(self, repo: str, issue_number: int, issue_title: str, error_msg: str):
        title = f"❌ 任务实施异常: #{issue_number} ({repo})"
        content = (
            f"- **目标仓库**: `{repo}`\n"
            f"- **关联 Issue**: `#{issue_number} - {issue_title}`\n"
            f"- **异常原因**: ```{error_msg[:300]}```\n"
            f"- **建议**: 请检查云端日志或介入人工排查"
        )
        self.notify(title, content)
