#!/usr/bin/env python3
"""
notifier.py 单元测试
"""

import unittest
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


if __name__ == "__main__":
    unittest.main()
