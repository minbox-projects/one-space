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
import urllib.error
import urllib.parse
import urllib.request
from typing import Any, Callable, Dict, Optional

logger = logging.getLogger("onespace-notifier")

MAX_DELIVERY_ATTEMPTS = 3


class Notifier:
    def __init__(
        self,
        config: Dict[str, Any],
        *,
        sleep: Optional[Callable[[float], None]] = None,
        clock: Optional[Callable[[], float]] = None,
        stop_event: Optional[Any] = None,
        deadline: Optional[Any] = None,
    ):
        notify_cfg = config.get("notification", {})
        self.enabled: bool = notify_cfg.get("enabled", False)
        self.dingtalk_url: Optional[str] = notify_cfg.get("dingtalk_webhook")
        self.dingtalk_secret: Optional[str] = notify_cfg.get("dingtalk_secret")
        self.wechat_url: Optional[str] = notify_cfg.get("wechat_work_webhook")

        self._sleep_override = sleep
        self._clock = clock or time.monotonic
        self._stop_event = stop_event
        self._deadline = deadline

    # ------------------------------------------------------------------
    # Cooperative control seams
    # ------------------------------------------------------------------
    def _remaining_budget(self) -> Optional[float]:
        if self._deadline is None:
            return None
        value = self._deadline() if callable(self._deadline) else self._deadline
        if value is None:
            return None
        return max(0.0, float(value) - float(self._clock()))

    def _sleep(self, seconds: float) -> None:
        if seconds <= 0:
            return
        remaining = self._remaining_budget()
        if remaining is not None:
            if remaining <= 0:
                return
            seconds = min(seconds, remaining)
        if self._sleep_override is not None:
            self._sleep_override(seconds)
        else:
            time.sleep(seconds)

    @staticmethod
    def _safe_url(url: str) -> str:
        try:
            parts = urllib.parse.urlsplit(url)
            host = parts.hostname or ""
            if parts.port:
                host = f"{host}:{parts.port}"
            return urllib.parse.urlunsplit((parts.scheme, host, parts.path, "", ""))
        except Exception:  # noqa: BLE001
            return "<redacted webhook>"

    # ------------------------------------------------------------------
    # Delivery
    # ------------------------------------------------------------------
    def _send_http_json(self, url: str, payload: Dict[str, Any]) -> bool:
        safe_url = self._safe_url(url)
        attempt = 0
        while attempt < MAX_DELIVERY_ATTEMPTS:
            remaining = self._remaining_budget()
            if remaining is not None and remaining <= 0:
                logger.warning(
                    "剩余关闭预算不足，跳过通知投递 [%s]", safe_url
                )
                return False
            timeout = 10 if remaining is None else max(0.1, min(10.0, remaining))
            attempt += 1
            try:
                data = json.dumps(payload, ensure_ascii=False).encode("utf-8")
                req = urllib.request.Request(
                    url,
                    data=data,
                    headers={"Content-Type": "application/json; charset=utf-8"},
                    method="POST",
                )
                with urllib.request.urlopen(req, timeout=timeout) as resp:
                    status = int(getattr(resp, "status", 200))
                    result = resp.read().decode("utf-8")
                    if status == 429 or 500 <= status < 600:
                        if attempt < MAX_DELIVERY_ATTEMPTS:
                            logger.warning(
                                "通知推送短暂失败 [%s] HTTP %s，准备重试",
                                safe_url, status,
                            )
                            self._sleep(2 ** (attempt - 1))
                            continue
                        logger.error("通知推送失败 [%s] HTTP %s", safe_url, status)
                        return False
                    if 200 <= status < 300:
                        logger.info("通知推送响应: %s", result[:120])
                        return True
                    logger.error("通知推送失败 [%s] HTTP %s", safe_url, status)
                    return False
            except urllib.error.HTTPError as e:
                code = getattr(e, "code", None)
                transient = code == 429 or (isinstance(code, int) and 500 <= code < 600)
                logger.error("通知推送失败 [%s]: HTTP %s", safe_url, code)
                if transient and attempt < MAX_DELIVERY_ATTEMPTS:
                    self._sleep(2 ** (attempt - 1))
                    continue
                return False
            except urllib.error.URLError as e:
                logger.error("通知推送失败 [%s]: %s", safe_url, e)
                if attempt < MAX_DELIVERY_ATTEMPTS:
                    self._sleep(2 ** (attempt - 1))
                    continue
                return False
            except Exception as e:  # noqa: BLE001 - transport errors are transient
                logger.error("通知推送失败 [%s]: %s", safe_url, e)
                if attempt < MAX_DELIVERY_ATTEMPTS:
                    self._sleep(2 ** (attempt - 1))
                    continue
                return False
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

    def send_dingtalk(self, title: str, markdown_content: str) -> bool:
        target_url = self.build_dingtalk_signed_url()
        if not target_url:
            return False
        payload = {
            "msgtype": "markdown",
            "markdown": {
                "title": title,
                "text": f"### {title}\n\n{markdown_content}\n\n> 🤖 来自 OneSpace Cloud Agent",
            },
        }
        return self._send_http_json(target_url, payload)

    def send_wechat(self, title: str, markdown_content: str) -> bool:
        if not self.wechat_url:
            return False
        payload = {
            "msgtype": "markdown",
            "markdown": {
                "content": f"### {title}\n{markdown_content}\n> 🤖 来自 OneSpace Cloud Agent",
            },
        }
        return self._send_http_json(self.wechat_url, payload)

    def notify(self, title: str, markdown_content: str) -> None:
        if not self.enabled:
            return
        remaining = self._remaining_budget()
        if remaining is not None and remaining <= 0:
            logger.warning("剩余关闭预算不足，跳过通知投递: %s", title)
            return
        if self.dingtalk_url:
            self.send_dingtalk(title, markdown_content)
        if self.wechat_url:
            self.send_wechat(title, markdown_content)

    def notify_task_started(self, repo: str, issue_number: int, issue_title: str) -> None:
        title = f"🚀 任务接单: #{issue_number} ({repo})"
        content = (
            f"- **目标仓库**: `{repo}`\n"
            f"- **Issue 编号**: `#{issue_number}`\n"
            f"- **需求标题**: **{issue_title}**\n"
            f"- **当前状态**: 云端 Agent 正在隔离环境中分析与实施..."
        )
        self.notify(title, content)

    def notify_task_completed(self, repo: str, issue_number: int, issue_title: str, pr_url: str) -> None:
        title = f"🎉 需求完成并生成 PR: #{issue_number} ({repo})"
        content = (
            f"- **目标仓库**: `{repo}`\n"
            f"- **关联 Issue**: `#{issue_number} - {issue_title}`\n"
            f"- **已生成 PR**: [点击查看 Pull Request]({pr_url})\n"
            f"- **后续步骤**: 请前往 GitHub 进行代码审查与合并"
        )
        self.notify(title, content)

    def notify_task_failed(self, repo: str, issue_number: int, issue_title: str, error_msg: str) -> None:
        title = f"❌ 任务实施异常: #{issue_number} ({repo})"
        content = (
            f"- **目标仓库**: `{repo}`\n"
            f"- **关联 Issue**: `#{issue_number} - {issue_title}`\n"
            f"- **异常原因**: ```{error_msg[:300]}```\n"
            f"- **建议**: 请检查云端日志或介入人工排查"
        )
        self.notify(title, content)
