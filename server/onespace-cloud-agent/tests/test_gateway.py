#!/usr/bin/env python3
"""
gateway.py 单元测试
"""

import json
import os
import tempfile
import unittest
from unittest.mock import patch
from core.gateway import GatewayEngine


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
        self.gateway = GatewayEngine(self.tmp_file.name, sync_opencode=False)

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

    def test_sync_to_opencode_config_provider_name(self):
        with tempfile.TemporaryDirectory() as tmp_dir:
            cfg_path = os.path.join(tmp_dir, "opencode.json")
            self.gateway.sync_to_opencode_config(target_file=cfg_path)
            self.assertTrue(os.path.exists(cfg_path))
            with open(cfg_path, "r", encoding="utf-8") as f:
                cfg = json.load(f)
            self.assertIn("provider", cfg)
            self.assertIn("gateway", cfg["provider"])
            self.assertEqual(cfg["provider"]["gateway"]["name"], "Cloud Agent Gateway")

    def test_sync_disabled_does_not_touch_disk(self):
        with tempfile.TemporaryDirectory() as tmp_home:
            with patch("os.path.expanduser", side_effect=lambda p: p.replace("~", tmp_home)):
                # sync_opencode=False 且未指定 target_file 时不产生任何文件
                self.gateway.sync_to_opencode_config()
                cfg_path = os.path.join(tmp_home, ".config", "opencode", "opencode.json")
                self.assertFalse(os.path.exists(cfg_path))


if __name__ == "__main__":
    unittest.main()


import threading


class TestOpencodeMerge(unittest.TestCase):
    def setUp(self):
        self.test_config = {"ai_gateway": {"port": 17688, "providers": {}, "model_pools": {}}}
        self.tmp_file = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(self.test_config, self.tmp_file)
        self.tmp_file.close()
        self.gateway = GatewayEngine(self.tmp_file.name, sync_opencode=False)

    def tearDown(self):
        if os.path.exists(self.tmp_file.name):
            os.remove(self.tmp_file.name)

    def test_merge_preserves_other_keys_and_jsonc(self):
        with tempfile.TemporaryDirectory() as d:
            cfg_path = os.path.join(d, "opencode.json")
            jsonc_path = os.path.join(d, "opencode.jsonc")
            with open(cfg_path, "w") as f:
                json.dump({"foo": 0, "bar": False, "provider": {"other": {"x": 1}, "gateway": {"old": True}}, "mcp": {"a": 1}}, f)
            jsonc_bytes = b'{"keep": true}\n'
            with open(jsonc_path, "wb") as f:
                f.write(jsonc_bytes)
            self.gateway.sync_to_opencode_config(target_file=cfg_path)
            with open(cfg_path) as f:
                cfg = json.load(f)
            self.assertEqual(cfg["foo"], 0)
            self.assertEqual(cfg["bar"], False)
            self.assertIn("other", cfg["provider"])
            self.assertIn("mcp", cfg)
            self.assertIn("gateway", cfg["provider"])
            # jsonc unchanged
            with open(jsonc_path, "rb") as f:
                self.assertEqual(f.read(), jsonc_bytes)

    def test_invalid_json_left_intact_no_temp(self):
        with tempfile.TemporaryDirectory() as d:
            cfg_path = os.path.join(d, "opencode.json")
            with open(cfg_path, "w") as f:
                f.write("{bad json")
            before = open(cfg_path, "rb").read()
            self.gateway.sync_to_opencode_config(target_file=cfg_path)
            after = open(cfg_path, "rb").read()
            self.assertEqual(before, after)
            # no temp files
            self.assertEqual([x for x in os.listdir(d) if x != "opencode.json"], [])

    def test_non_object_left_intact(self):
        with tempfile.TemporaryDirectory() as d:
            cfg_path = os.path.join(d, "opencode.json")
            with open(cfg_path, "w") as f:
                json.dump([1, 2, 3], f)
            before = open(cfg_path, "rb").read()
            self.gateway.sync_to_opencode_config(target_file=cfg_path)
            after = open(cfg_path, "rb").read()
            self.assertEqual(before, after)

    def test_injected_pre_replace_failure_preserves_original_and_removes_temp(self):
        with tempfile.TemporaryDirectory() as d:
            cfg_path = os.path.join(d, "opencode.json")
            with open(cfg_path, "w") as f:
                json.dump({"keep": "me"}, f)
            before = open(cfg_path, "rb").read()
            # inject failure into os.replace
            import core.gateway as g
            orig = os.replace
            def boom(*a, **k):
                raise OSError("boom")
            with patch("os.replace", side_effect=boom):
                self.gateway.sync_to_opencode_config(target_file=cfg_path)
            after = open(cfg_path, "rb").read()
            self.assertEqual(before, after)
            self.assertEqual([x for x in os.listdir(d) if x.startswith("opencode.json") and x != "opencode.json"], [])

    def test_concurrent_sync_serializes(self):
        with tempfile.TemporaryDirectory() as d:
            cfg_path = os.path.join(d, "opencode.json")
            with open(cfg_path, "w") as f:
                json.dump({"seed": 1}, f)
            barrier = threading.Barrier(4)
            def worker():
                barrier.wait()
                self.gateway.sync_to_opencode_config(target_file=cfg_path)
            ts = [threading.Thread(target=worker) for _ in range(4)]
            for t in ts: t.start()
            for t in ts: t.join()
            # valid json
            with open(cfg_path) as f:
                cfg = json.load(f)
            self.assertIn("provider", cfg)


class TestCatalogConsistency(unittest.TestCase):
    def setUp(self):
        self.test_config = {"ai_gateway": {"port": 17688, "providers": {"p": {"base_url": "x", "api_keys": ["k"], "models": {"m": {"upstream_model": "u"}}, "is_active": True, "template_url": "http://example.invalid/t"}}, "model_pools": {}}}
        self.tmp_file = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(self.test_config, self.tmp_file)
        self.tmp_file.close()
        self.gateway = GatewayEngine(self.tmp_file.name, sync_opencode=False)

    def tearDown(self):
        if os.path.exists(self.tmp_file.name):
            os.remove(self.tmp_file.name)

    def test_readers_do_not_crash_during_sync_templates(self):
        # stub network
        import urllib.request
        def fake_urlopen(req, timeout=15):
            class R:
                status = 200
                def read(self):
                    return b'{"data": [{"id": "newm"}]}'
                def __enter__(self): return self
                def __exit__(self, *a): return False
            return R()
        with patch("urllib.request.urlopen", side_effect=fake_urlopen):
            errors = []
            def reader():
                try:
                    for _ in range(50):
                        self.gateway.resolve_candidates("m")
                        self.gateway.resolve_candidates("p/newm")
                except Exception as e:
                    errors.append(e)
            def writer():
                try:
                    for _ in range(5):
                        self.gateway.sync_templates()
                except Exception as e:
                    errors.append(e)
            ts = [threading.Thread(target=reader) for _ in range(4)] + [threading.Thread(target=writer)]
            for t in ts: t.start()
            for t in ts: t.join()
            self.assertEqual(errors, [])


import threading
import time


class TestCatalogGenerationAtomicity(unittest.TestCase):
    def setUp(self):
        self.test_config = {
            "ai_gateway": {
                "port": 17688,
                "providers": {
                    "p": {
                        "base_url": "x",
                        "api_keys": ["k"],
                        "models": {"m": {"upstream_model": "u"}},
                        "is_active": True,
                        "template_url": "http://example.invalid/t",
                    }
                },
                "model_pools": {
                    "pool": {"description": "d", "candidates": [{"provider": "p", "model": "m"}]}
                },
            }
        }
        self.tmp_file = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(self.test_config, self.tmp_file)
        self.tmp_file.close()
        self.gateway = GatewayEngine(self.tmp_file.name, sync_opencode=False)

    def tearDown(self):
        if os.path.exists(self.tmp_file.name):
            os.remove(self.tmp_file.name)

    def test_reader_sees_complete_generation(self):
        fetch_started = threading.Event()
        fetch_release = threading.Event()

        def fake_urlopen(req, timeout=15):
            fetch_started.set()
            fetch_release.wait(timeout=5)
            class R:
                status = 200
                def read(self):
                    return b'{"data": [{"id": "newm"}]}'
                def __enter__(self):
                    return self
                def __exit__(self, *a):
                    return False
            return R()

        observed = []
        errors = []
        stop = threading.Event()

        def reader():
            while not stop.is_set():
                try:
                    cands = self.gateway.resolve_candidates("pool")
                    models = tuple(sorted(self.gateway.providers["p"].models.keys()))
                    observed.append((tuple(cands), models))
                except Exception as e:
                    errors.append(e)
                time.sleep(0.001)

        with patch("urllib.request.urlopen", side_effect=fake_urlopen):
            ts = [threading.Thread(target=reader) for _ in range(4)]
            for t in ts:
                t.start()
            time.sleep(0.05)
            w = threading.Thread(target=self.gateway.sync_templates)
            w.start()
            fetch_started.wait(timeout=5)
            # readers collect while fetch is blocked -> complete old generation
            time.sleep(0.05)
            fetch_release.set()
            w.join(timeout=5)
            time.sleep(0.05)
            stop.set()
            for t in ts:
                t.join(timeout=5)

        self.assertEqual(errors, [])
        self.assertGreater(len(observed), 0)
        for obs in observed:
            self.assertIn(obs[1], (("m",), ("m", "newm")), msg=str(obs))
        model_sets = {o[1] for o in observed}
        self.assertIn(("m",), model_sets)
        self.assertIn(("m", "newm"), model_sets)

    def test_opencode_projection_one_snapshot(self):
        cfg = {
            "ai_gateway": {
                "port": 17688,
                "providers": {
                    "p": {
                        "base_url": "x",
                        "api_keys": ["k"],
                        "models": {"m": {"upstream_model": "u"}},
                        "is_active": True,
                    }
                },
                "model_pools": {},
            }
        }
        tmpf = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(cfg, tmpf)
        tmpf.close()
        gw = GatewayEngine(tmpf.name, sync_opencode=False)
        os.remove(tmpf.name)

        expected_new = set()
        for pool_id in gw.model_pools:
            expected_new.add(pool_id)
        for p_name, provider in gw.providers.items():
            if provider.is_active:
                for m_id in provider.models:
                    if m_id not in expected_new:
                        expected_new.add(m_id)
        expected_new = frozenset(expected_new)

        with tempfile.TemporaryDirectory() as d:
            cfg_path = os.path.join(d, "opencode.json")
            with open(cfg_path, "w", encoding="utf-8") as f:
                json.dump({"seed": 1}, f)

            observations = []
            errors = []
            stop = threading.Event()

            def writer():
                gw.sync_to_opencode_config(target_file=cfg_path)

            def reader():
                while not stop.is_set():
                    try:
                        with open(cfg_path, "r", encoding="utf-8") as f:
                            data = json.load(f)
                        gw_section = {}
                        if isinstance(data.get("provider"), dict):
                            gw_section = data["provider"].get("gateway", {}) or {}
                        if gw_section:
                            models = frozenset((gw_section.get("models") or {}).keys())
                        else:
                            models = None
                        observations.append(models)
                    except Exception as e:
                        errors.append(e)
                    time.sleep(0.001)

            ts = [threading.Thread(target=reader) for _ in range(4)]
            for t in ts:
                t.start()
            time.sleep(0.02)
            w = threading.Thread(target=writer)
            w.start()
            w.join(timeout=5)
            time.sleep(0.05)
            stop.set()
            for t in ts:
                t.join(timeout=5)

        self.assertEqual(errors, [])
        for obs in observations:
            self.assertIn(obs, (None, expected_new), msg=str(obs))


if __name__ == "__main__":
    unittest.main()
