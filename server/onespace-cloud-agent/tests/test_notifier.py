#!/usr/bin/env python3
"""
notifier.py 单元测试
"""

import unittest
from unittest import mock
import urllib.error
import urllib.request
from core.notifier import Notifier


class TestNotifier(unittest.TestCase):
    def test_dingtalk_signing(self):
        cfg = {
            "notification": {
                "enabled": True,
                "dingtalk_webhook": "https://oapi.dingtalk.com/robot/send?access_token=dummy",
                "dingtalk_secret": "my_secret_key"
            }
        }
        notifier = Notifier(cfg)
        signed_url = notifier.build_dingtalk_signed_url(timestamp="1600000000000")
        self.assertIn("timestamp=1600000000000", signed_url)
        self.assertIn("sign=", signed_url)


class TestNotifierRetry(unittest.TestCase):
    def test_transient_failure_retries_three_attempts(self):
        calls = {"n": 0}
        sleeps = []

        def boom(*a, **k):
            calls["n"] += 1
            raise urllib.error.URLError("boom")

        with mock.patch("urllib.request.urlopen", side_effect=boom):
            cfg = {"notification": {"enabled": True, "dingtalk_webhook": "https://example.com/hook", "dingtalk_secret": None}}
            notifier = Notifier(cfg)
            with mock.patch("time.sleep", side_effect=lambda s: sleeps.append(s)):
                notifier.send_dingtalk("t", "c")
        self.assertEqual(calls["n"], 3)
        self.assertEqual(sleeps, [1, 2])

    def test_permanent_4xx_once(self):
        calls = {"n": 0}

        def boom(*a, **k):
            calls["n"] += 1
            raise urllib.error.HTTPError("u", 403, "forbidden", {}, None)

        with mock.patch("urllib.request.urlopen", side_effect=boom):
            cfg = {"notification": {"enabled": True, "dingtalk_webhook": "https://example.com/hook?token=secret", "dingtalk_secret": None}}
            notifier = Notifier(cfg)
            with mock.patch("time.sleep"):
                notifier.send_dingtalk("t", "c")
        self.assertEqual(calls["n"], 1)

    def test_channels_independent(self):
        calls = {"ding": 0, "wechat": 0}

        def boom(*a, **k):
            url = a[0].full_url if hasattr(a[0], "full_url") else str(a[0])
            if "ding" in url:
                calls["ding"] += 1
                raise urllib.error.URLError("boom")
            calls["wechat"] += 1
            return mock.MagicMock(status=200, read=lambda: b"ok")

        with mock.patch("urllib.request.urlopen", side_effect=boom):
            cfg = {"notification": {"enabled": True, "dingtalk_webhook": "https://example.com/ding", "wechat_work_webhook": "https://example.com/wechat", "dingtalk_secret": None}}
            notifier = Notifier(cfg)
            with mock.patch("time.sleep"):
                notifier.notify("t", "c")
        self.assertEqual(calls["ding"], 3)
        self.assertEqual(calls["wechat"], 1)

    def test_webhook_credential_redacted_from_error_logs(self):
        def boom(*a, **k):
            raise urllib.error.URLError("boom")

        with mock.patch("urllib.request.urlopen", side_effect=boom):
            cfg = {"notification": {"enabled": True, "dingtalk_webhook": "https://example.com/hook?access_token=SECRET", "dingtalk_secret": None}}
            notifier = Notifier(cfg)
            with mock.patch("time.sleep"):
                with self.assertLogs("onespace-notifier", level="ERROR") as cm:
                    notifier.send_dingtalk("t", "c")
        joined = "\n".join(cm.output)
        self.assertNotIn("SECRET", joined)


class TestNotifierDeadlineSkip(unittest.TestCase):
    def test_expired_budget_skips_delivery_without_http(self):
        calls = {"n": 0}

        def responder(*a, **k):
            calls["n"] += 1
            return mock.MagicMock(status=200, read=lambda: b"ok")

        with mock.patch("urllib.request.urlopen", side_effect=responder) as urlopen:
            cfg = {
                "notification": {
                    "enabled": True,
                    "dingtalk_webhook": "https://example.com/hook",
                    "wechat_work_webhook": "https://example.com/wechat",
                    "dingtalk_secret": None,
                }
            }
            notifier = Notifier(cfg, clock=lambda: 100.0, deadline=lambda: 50.0)
            with self.assertLogs("onespace-notifier", level="WARNING") as cm:
                notifier.notify("t", "c")
        self.assertEqual(urlopen.call_count, 0)
        self.assertTrue(any("跳过通知投递" in m for m in cm.output))
        self.assertEqual(calls["n"], 0)


if __name__ == "__main__":
    unittest.main()
