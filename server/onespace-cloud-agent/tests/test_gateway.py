#!/usr/bin/env python3
"""
gateway.py 单元测试
"""

import json
import os
import tempfile
import unittest
from core.gateway import GatewayEngine, ProviderState


class TestGateway(unittest.TestCase):
    def setUp(self):
        self.test_config = {
            "ai_gateway": {
                "port": 17688,
                "auto_sync_interval_seconds": 3600,
                "providers": {
                    "provider_a": {
                        "base_url": "https://api.a.com/v1",
                        "api_keys": ["key_a1", "key_a2"],
                        "models": {
                            "model_a1": {"upstream_model": "upstream_a1"}
                        },
                        "is_active": True
                    },
                    "provider_b": {
                        "base_url": "https://api.b.com/v1",
                        "api_keys": ["key_b1"],
                        "models": {
                            "model_b1": {"upstream_model": "upstream_b1"}
                        },
                        "is_active": True
                    }
                },
                "model_pools": {
                    "smart-coder": {
                        "description": "智能编程候选池",
                        "candidates": [
                            {"provider": "provider_a", "model": "model_a1"},
                            {"provider": "provider_b", "model": "model_b1"}
                        ]
                    }
                }
            }
        }
        self.tmp_file = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(self.test_config, self.tmp_file)
        self.tmp_file.close()
        self.gateway = GatewayEngine(self.tmp_file.name)

    def tearDown(self):
        if os.path.exists(self.tmp_file.name):
            os.remove(self.tmp_file.name)

    def test_key_rotation_and_cooldown(self):
        p = self.gateway.providers["provider_a"]
        k1 = p.get_available_key()
        k2 = p.get_available_key()
        self.assertEqual(k1, "key_a1")
        self.assertEqual(k2, "key_a2")

        # 冷却 key_a1
        p.mark_key_failed("key_a1", cooldown_seconds=60)
        # 再次获取，应该跳过已冷却的 key_a1，只返回 key_a2
        k3 = p.get_available_key()
        self.assertEqual(k3, "key_a2")

    def test_model_pool_resolution(self):
        candidates = self.gateway.resolve_candidates("smart-coder")
        self.assertEqual(len(candidates), 2)
        self.assertEqual(candidates[0], ("provider_a", "upstream_a1"))
        self.assertEqual(candidates[1], ("provider_b", "upstream_b1"))

    def test_direct_prefix_resolution(self):
        candidates = self.gateway.resolve_candidates("provider_a/custom-model")
        self.assertEqual(len(candidates), 1)
        self.assertEqual(candidates[0], ("provider_a", "custom-model"))

    def test_mapped_model_resolution(self):
        candidates = self.gateway.resolve_candidates("model_a1")
        self.assertTrue(any(c == ("provider_a", "upstream_a1") for c in candidates))


if __name__ == "__main__":
    unittest.main()
