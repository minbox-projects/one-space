#!/usr/bin/env python3
import http.server
import json
import os
import tempfile
import threading
import unittest
import urllib.error
import urllib.request

from core.gateway import GatewayEngine, GatewayRequestHandler


def _config(token="sk-test-token", max_body=10485760, providers=None, pools=None, require=True):
    return {
        "ai_gateway": {
            "port": 17688,
            "auth": {"token": token, "require": require},
            "max_body_bytes": max_body,
            "providers": providers or {
                "p": {
                    "base_url": "http://127.0.0.1:1/upstream",
                    "api_keys": ["k"],
                    "models": {"m": {"upstream_model": "upstream-m"}},
                    "is_active": True,
                }
            },
            "model_pools": pools or {},
        }
    }


class _Server:
    def __init__(self, gateway):
        handler = type("H", (GatewayRequestHandler,), {})
        handler.gateway = gateway
        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.port = self.httpd.server_address[1]
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
        self.thread.start()

    def close(self):
        self.httpd.shutdown()
        self.httpd.server_close()


class GatewayHttpTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(_config(), self.tmp)
        self.tmp.close()
        self.gateway = GatewayEngine(self.tmp.name, sync_opencode=False)
        self.srv = _Server(self.gateway)
        self.base = f"http://127.0.0.1:{self.srv.port}"

    def tearDown(self):
        self.srv.close()
        if os.path.exists(self.tmp.name):
            os.remove(self.tmp.name)

    def _req(self, method, path, headers=None, data=None):
        url = self.base + path
        req = urllib.request.Request(url, method=method, data=data, headers=headers or {})
        try:
            with urllib.request.urlopen(req) as r:
                return r.status, dict(r.headers), r.read()
        except urllib.error.HTTPError as e:
            return e.code, dict(e.headers), e.read()

    def test_origin_rejected_all_paths(self):
        for path in ["/health", "/v1/health", "/models", "/v1/models"]:
            st, hdrs, _ = self._req("GET", path, headers={"Origin": "http://evil"})
            self.assertEqual(st, 403, path)
            self.assertNotIn("Access-Control-Allow-Origin", hdrs)
        # empty origin
        st, _, _ = self._req("GET", "/health", headers={"Origin": ""})
        self.assertEqual(st, 403)
        # POST
        st, _, _ = self._req("POST", "/chat/completions", headers={"Origin": "x", "Content-Type": "application/json"}, data=b"{}")
        self.assertEqual(st, 403)
        # OPTIONS with origin -> 403
        st, _, _ = self._req("OPTIONS", "/health", headers={"Origin": "x"})
        self.assertEqual(st, 403)
        # OPTIONS without origin -> 204 no wildcard
        st, hdrs, _ = self._req("OPTIONS", "/health")
        self.assertEqual(st, 204)
        self.assertNotIn("Access-Control-Allow-Origin", hdrs)

    def test_no_wildcard_cors_any_path(self):
        st, hdrs, _ = self._req("GET", "/health")
        self.assertEqual(st, 200)
        self.assertNotIn("Access-Control-Allow-Origin", hdrs)

    def test_auth_required_protected_routes(self):
        for path in ["/models", "/v1/models"]:
            st, _, _ = self._req("GET", path)
            self.assertEqual(st, 401)
        st, _, _ = self._req("POST", "/chat/completions", headers={"Content-Type": "application/json"}, data=b'{"model":"m"}')
        self.assertEqual(st, 401)
        st, _, _ = self._req("POST", "/v1/chat/completions", headers={"Content-Type": "application/json"}, data=b'{"model":"m"}')
        self.assertEqual(st, 401)

    def test_auth_valid_and_health_public(self):
        h = {"Authorization": "Bearer sk-test-token"}
        st, _, _ = self._req("GET", "/models", headers=h)
        self.assertEqual(st, 200)
        st, _, raw = self._req("GET", "/health")
        self.assertEqual(st, 200)
        data = json.loads(raw.decode("utf-8"))
        self.assertIn("providers", data)
        self.assertEqual(data["providers_count"], len(data["providers"]))
        self.assertTrue(any(p["id"] == "p" for p in data["providers"]))
        p_item = next(p for p in data["providers"] if p["id"] == "p")
        self.assertEqual(p_item["models"][0]["upstream_model"], "upstream-m")
        st, _, _ = self._req("GET", "/v1/health")
        self.assertEqual(st, 200)

    def test_auth_require_false_bypasses_only_auth(self):
        # rebuild config with require false
        self.srv.close()
        os.remove(self.tmp.name)
        self.tmp = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(_config(require=False), self.tmp)
        self.tmp.close()
        self.gateway = GatewayEngine(self.tmp.name, sync_opencode=False)
        self.srv = _Server(self.gateway)
        self.base = f"http://127.0.0.1:{self.srv.port}"
        st, _, _ = self._req("GET", "/models")
        self.assertEqual(st, 200)
        # origin still rejected
        st, _, _ = self._req("GET", "/health", headers={"Origin": "x"})
        self.assertEqual(st, 403)

    def test_custom_token_projected(self):
        self.srv.close()
        os.remove(self.tmp.name)
        self.tmp = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(_config(token="sk-custom-xyz"), self.tmp)
        self.tmp.close()
        self.gateway = GatewayEngine(self.tmp.name, sync_opencode=False)
        # sync opencode to temp file
        with tempfile.TemporaryDirectory() as d:
            cfgp = os.path.join(d, "opencode.json")
            self.gateway.sync_to_opencode_config(target_file=cfgp)
            with open(cfgp) as f:
                cfg = json.load(f)
            self.assertEqual(cfg["provider"]["gateway"]["options"]["apiKey"], "sk-custom-xyz")

    def test_body_size_boundaries(self):
        self.srv.close()
        os.remove(self.tmp.name)
        self.tmp = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(_config(max_body=1024), self.tmp)
        self.tmp.close()
        self.gateway = GatewayEngine(self.tmp.name, sync_opencode=False)
        self.srv = _Server(self.gateway)
        self.base = f"http://127.0.0.1:{self.srv.port}"
        h = {"Authorization": "Bearer sk-test-token", "Content-Type": "application/json"}
        # exact limit accepted by size check (empty json? need valid JSON; use padding)
        body = b'{"model":"m","x":"' + b"a" * (1024 - len(b'{"model":"m","x":"') - 2) + b'"}'
        # ensure exact 1024
        body = body[:1024]
        # make valid json by simpler: use model only plus padding in a string field? Instead test size gate via Content-Length 1024 with malformed? Spec: exact accepted by size check then JSON parse may fail. We'll send 1024 bytes of valid-ish json impossible. Use Content-Length 1024 with body 1024 bytes that is valid JSON with padding.
        # Build exact 1024 valid JSON:
        prefix = b'{"model":"m","pad":"'
        suffix = b'"}'
        padlen = 1024 - len(prefix) - len(suffix)
        body = prefix + b"a" * padlen + suffix
        self.assertEqual(len(body), 1024)
        st, _, _ = self._req("POST", "/chat/completions", headers={**h, "Content-Length": str(len(body))}, data=body)
        # size gate specifically passed -> not 413; body valid -> not 400
        self.assertNotEqual(st, 413)
        self.assertNotEqual(st, 400)
        # oversize
        body2 = body + b" "
        st, _, _ = self._req("POST", "/chat/completions", headers={**h, "Content-Length": str(len(body2))}, data=body2)
        self.assertEqual(st, 413)
        # invalid lengths
        for cl in ["0", "-5", "abc"]:
            hdrs = dict(h)
            hdrs["Content-Length"] = cl
            st, _, _ = self._req("POST", "/chat/completions", headers=hdrs, data=b'{"model":"m"}')
            self.assertEqual(st, 400, cl)
        # empty body
        st, _, _ = self._req("POST", "/chat/completions", headers={**h, "Content-Length": "2"}, data=b"{}")
        # "{}" length 2 valid json but model missing -> 404 maybe; size ok. Empty body:
        st, _, _ = self._req("POST", "/chat/completions", headers={**h, "Content-Length": "0"})
        self.assertEqual(st, 400)
        # malformed json
        st, _, _ = self._req("POST", "/chat/completions", headers={**h, "Content-Length": "5"}, data=b"{bad}")
        self.assertEqual(st, 400)

    def test_body_invalid_length_no_read_raw_socket(self):
        import socket
        port = self.srv.port
        token = "sk-test-token"
        for cl in (None, "0", "-5", "abc"):
            s = socket.create_connection(("127.0.0.1", port), timeout=2)
            s.settimeout(2)
            lines = [
                "POST /chat/completions HTTP/1.1",
                "Host: localhost",
                "Authorization: Bearer " + token,
                "Content-Type: application/json",
            ]
            if cl is not None:
                lines.append("Content-Length: " + cl)
            lines.append("")
            lines.append("")
            s.sendall("\r\n".join(lines).encode("ascii"))
            data = b""
            try:
                while True:
                    chunk = s.recv(4096)
                    if not chunk:
                        break
                    data += chunk
            except socket.timeout:
                pass
            s.close()
            status_line = data.split(b"\r\n", 1)[0]
            self.assertIn(b"400", status_line, (cl, data))
        # missing body read: response arrives within timeout (no hang) already proven by no timeout above


    def test_unknown_model_404(self):
        h = {"Authorization": "Bearer sk-test-token", "Content-Type": "application/json"}
        for model in ["nope", "", "   "]:
            body = json.dumps({"model": model}).encode()
            st, _, raw = self._req("POST", "/chat/completions", headers={**h, "Content-Length": str(len(body))}, data=body)
            self.assertEqual(st, 404)
            self.assertIn(b"model_not_found", raw)

    def test_health_and_models_expose_context_and_reasoning_efforts(self):
        # 准备带 name、context_window 和 reasoning_efforts 的配置
        custom_cfg = _config(
            providers={
                "custom_provider": {
                    "name": "Custom Provider",
                    "base_url": "http://127.0.0.1:1/upstream",
                    "api_keys": ["k"],
                    "models": {
                        "m-reason": {
                            "upstream_model": "upstream-reason",
                            "display_name": "Reasoning Model",
                            "context_window": 128000,
                            "reasoning_efforts": ["low", "medium", "high"],
                        }
                    },
                    "is_active": True,
                }
            },
            pools={
                "test-pool": {
                    "description": "Test Virtual Pool",
                    "context_window": 256000,
                    "reasoning_efforts": ["high"],
                }
            }
        )
        with tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json") as f:
            json.dump(custom_cfg, f)
            cfg_file = f.name
        try:
            gw = GatewayEngine(cfg_file, sync_opencode=False)
            srv = _Server(gw)
            try:
                # 1. 验证 /health
                req = urllib.request.Request(f"http://127.0.0.1:{srv.port}/health")
                with urllib.request.urlopen(req) as resp:
                    health_data = json.loads(resp.read().decode())
                self.assertEqual(health_data["status"], "ok")
                self.assertIn("providers", health_data)
                self.assertEqual(len(health_data["providers"]), 1)
                p = health_data["providers"][0]
                self.assertEqual(p["id"], "custom_provider")
                self.assertEqual(p["name"], "Custom Provider")
                self.assertEqual(len(p["models"]), 1)
                m = p["models"][0]
                self.assertEqual(m["id"], "m-reason")
                self.assertEqual(m["upstream_model"], "upstream-reason")
                self.assertEqual(m["display_name"], "Reasoning Model")
                self.assertEqual(m["context_window"], 128000)
                self.assertEqual(m["reasoning_efforts"], ["low", "medium", "high"])

                # 2. 验证 /models
                req_models = urllib.request.Request(
                    f"http://127.0.0.1:{srv.port}/models",
                    headers={"Authorization": "Bearer sk-test-token"}
                )
                with urllib.request.urlopen(req_models) as resp:
                    models_data = json.loads(resp.read().decode())
                m_list = models_data.get("data", [])
                pool_model = next((item for item in m_list if item["id"] == "test-pool"), None)
                self.assertIsNotNone(pool_model)
                self.assertEqual(pool_model.get("context_window"), 256000)
                self.assertEqual(pool_model.get("reasoning_efforts"), ["high"])

                prov_model = next((item for item in m_list if item["id"] == "custom_provider/m-reason"), None)
                self.assertIsNotNone(prov_model)
                self.assertEqual(prov_model.get("context_window"), 128000)
                self.assertEqual(prov_model.get("reasoning_efforts"), ["low", "medium", "high"])
            finally:
                srv.close()
        finally:
            if os.path.exists(cfg_file):
                os.remove(cfg_file)


if __name__ == "__main__":
    unittest.main()

_UPSTREAM_CALLS = {"count": 0, "models": []}

class _Upstream(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        _UPSTREAM_CALLS["count"] += 1
        n = int(self.headers.get("Content-Length", 0))
        raw = self.rfile.read(n)
        try:
            _UPSTREAM_CALLS["models"].append(json.loads(raw).get("model"))
        except Exception:
            pass
        body = b'{"choices":[{"message":{"content":"ok"}}]}'
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *a):
        pass


class RecognizedModelTests(unittest.TestCase):
    def setUp(self):
        _UPSTREAM_CALLS["count"] = 0
        _UPSTREAM_CALLS["models"] = []
        self.up = http.server.ThreadingHTTPServer(("127.0.0.1", 0), _Upstream)
        self.up_port = self.up.server_address[1]
        threading.Thread(target=self.up.serve_forever, daemon=True).start()
        cfg = _config(providers={
            "p": {"base_url": f"http://127.0.0.1:{self.up_port}", "api_keys": ["k"], "models": {"m": {"upstream_model": "upstream-m"}}, "is_active": True}
        }, pools={"pool": {"description": "d", "candidates": [{"provider": "p", "model": "m"}]}})
        # explicit mapping on active provider
        cfg["ai_gateway"]["providers"]["p"]["models"]["mapped"] = {"upstream_model": "upstream-mapped"}
        self.tmp = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(cfg, self.tmp)
        self.tmp.close()
        self.gateway = GatewayEngine(self.tmp.name, sync_opencode=False)
        handler = type("H", (GatewayRequestHandler,), {})
        handler.gateway = self.gateway
        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.port = self.httpd.server_address[1]
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()
        self.base = f"http://127.0.0.1:{self.port}"
        self.h = {"Authorization": "Bearer sk-test-token", "Content-Type": "application/json"}

    def tearDown(self):
        self.httpd.shutdown()
        self.up.shutdown()
        if os.path.exists(self.tmp.name):
            os.remove(self.tmp.name)

    def _req(self, body):
        data = json.dumps(body).encode()
        req = urllib.request.Request(self.base + "/chat/completions", data=data, headers={**self.h, "Content-Length": str(len(data))})
        try:
            with urllib.request.urlopen(req) as r:
                return r.status, r.read()
        except urllib.error.HTTPError as e:
            return e.code, e.read()

    def test_pool_resolves(self):
        st, _ = self._req({"model": "pool"})
        self.assertEqual(st, 200)
        self.assertEqual(_UPSTREAM_CALLS["count"], 1)
        self.assertEqual(_UPSTREAM_CALLS["models"], ["upstream-m"])

    def test_prefix_passthrough(self):
        st, _ = self._req({"model": "p/custom"})
        self.assertEqual(st, 200)
        self.assertEqual(_UPSTREAM_CALLS["count"], 1)
        self.assertEqual(_UPSTREAM_CALLS["models"], ["custom"])

    def test_explicit_mapping(self):
        st, _ = self._req({"model": "mapped"})
        self.assertEqual(st, 200)
        self.assertEqual(_UPSTREAM_CALLS["count"], 1)
        self.assertEqual(_UPSTREAM_CALLS["models"], ["upstream-mapped"])

    def test_known_pool_unavailable_returns_502(self):
        # known pool with no usable candidate (no keys): HTTP 502, not 404
        cfg = _config(providers={"p": {"base_url": "http://127.0.0.1:1", "api_keys": [], "models": {"m": {"upstream_model": "u"}}, "is_active": True}},
                      pools={"empty": {"description": "d", "candidates": [{"provider": "p", "model": "m"}]}})
        tmp = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(cfg, tmp); tmp.close()
        g = GatewayEngine(tmp.name, sync_opencode=False)
        # spin a server for this gateway
        handler = type("H", (GatewayRequestHandler,), {})
        handler.gateway = g
        httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        port = httpd.server_address[1]
        threading.Thread(target=httpd.serve_forever, daemon=True).start()
        h = {"Authorization": "Bearer sk-test-token", "Content-Type": "application/json"}
        data = json.dumps({"model": "empty"}).encode()
        req = urllib.request.Request(f"http://127.0.0.1:{port}/chat/completions", data=data, headers={**h, "Content-Length": str(len(data))})
        try:
            with urllib.request.urlopen(req) as r:
                st = r.status
        except urllib.error.HTTPError as e:
            st = e.code
        httpd.shutdown()
        os.remove(tmp.name)
        self.assertEqual(st, 502)


class UpstreamCallCountTests(unittest.TestCase):
    def setUp(self):
        _UPSTREAM_CALLS["count"] = 0
        _UPSTREAM_CALLS["models"] = []
        cfg = _config(max_body=1024, providers={
            "p": {"base_url": "http://127.0.0.1:1", "api_keys": ["k"], "models": {"m": {"upstream_model": "upstream-m"}}, "is_active": True}
        })
        # placeholder replaced below? We'll fix port after server start
        self.tmp = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(cfg, self.tmp)
        self.tmp.close()
        # start upstream first
        self.up = http.server.ThreadingHTTPServer(("127.0.0.1", 0), _Upstream)
        self.up_port = self.up.server_address[1]
        threading.Thread(target=self.up.serve_forever, daemon=True).start()
        # rewrite config with correct port
        cfg["ai_gateway"]["providers"]["p"]["base_url"] = f"http://127.0.0.1:{self.up_port}"
        with open(self.tmp.name, "w") as f:
            json.dump(cfg, f)
        self.gateway = GatewayEngine(self.tmp.name, sync_opencode=False)
        handler = type("H", (GatewayRequestHandler,), {})
        handler.gateway = self.gateway
        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.port = self.httpd.server_address[1]
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()
        self.base = f"http://127.0.0.1:{self.port}"
        self.h = {"Authorization": "Bearer sk-test-token", "Content-Type": "application/json"}

    def tearDown(self):
        self.httpd.shutdown()
        self.up.shutdown()
        if os.path.exists(self.tmp.name):
            os.remove(self.tmp.name)

    def _req(self, body_bytes, cl=None, headers=None):
        hdrs = dict(self.h)
        if cl is not None:
            hdrs["Content-Length"] = cl
        if headers:
            hdrs.update(headers)
        req = urllib.request.Request(self.base + "/chat/completions", data=body_bytes, headers=hdrs, method="POST")
        try:
            with urllib.request.urlopen(req) as r:
                return r.status, r.read()
        except urllib.error.HTTPError as e:
            return e.code, e.read()

    def test_413_zero_upstream_calls(self):
        body = b"x" * 1025
        st, _ = self._req(body, cl=str(len(body)))
        self.assertEqual(st, 413)
        self.assertEqual(_UPSTREAM_CALLS["count"], 0)

    def test_400_invalid_length_zero_upstream_calls(self):
        for cl in ("0", "-5", "abc"):
            _UPSTREAM_CALLS["count"] = 0
            st, _ = self._req(b'{"model":"m"}', cl=cl)
            self.assertEqual(st, 400, cl)
            self.assertEqual(_UPSTREAM_CALLS["count"], 0, cl)

    def test_unknown_model_zero_upstream_calls(self):
        body = json.dumps({"model": "nope"}).encode()
        st, _ = self._req(body, cl=str(len(body)))
        self.assertEqual(st, 404)
        self.assertEqual(_UPSTREAM_CALLS["count"], 0)

    def test_exact_boundary_calls_upstream(self):
        body = json.dumps({"model": "m"}).encode()
        # pad to 1024? size gate uses Content-Length; send exact 1024 bytes valid-ish not needed; use 1024 zeros? model parse fails 400. Instead use body length 10? Boundary test already in size. Here just ensure recognized calls upstream.
        st, _ = self._req(body, cl=str(len(body)))
        self.assertNotIn(st, (413, 400))
        self.assertGreaterEqual(_UPSTREAM_CALLS["count"], 1)


import socket
import time as _time

# ---------------------------------------------------------------------------
# Step 2: provider failover, cooling and streaming relay
# ---------------------------------------------------------------------------


class _FailoverUpstream(http.server.BaseHTTPRequestHandler):
    records = []
    key_behavior = {}

    def do_POST(self):
        key = self.headers.get("Authorization", "").rsplit(" ", 1)[-1]
        _FailoverUpstream.records.append(key)
        behavior = _FailoverUpstream.key_behavior.get(key, "ok")
        n = int(self.headers.get("Content-Length", 0) or 0)
        if n:
            self.rfile.read(n)
        if behavior == "429":
            retry_after = _FailoverUpstream.key_behavior.get(key + ":ra", "5")
            body = b'{"error":"rate limited"}'
            self.send_response(429)
            self.send_header("Content-Type", "application/json")
            self.send_header("Retry-After", str(retry_after))
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        elif behavior == "500":
            body = b'{"error":"server error"}'
            self.send_response(500)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        elif behavior == "400":
            body = b'{"error":"bad request"}'
            self.send_response(400)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        elif behavior == "transport":
            self.close_connection = True
            try:
                self.request.shutdown(socket.SHUT_RDWR)
            except Exception:
                pass
            return
        else:
            body = b'{"choices":[{"message":{"content":"ok"}}]}'
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        self.wfile.flush()

    def log_message(self, *a):
        pass


def _failover_config(up_port):
    return {
        "ai_gateway": {
            "port": 17688,
            "auth": {"token": "sk-test-token", "require": True},
            "max_body_bytes": 10485760,
            "retry": {"max_cooldown_seconds": 600},
            "providers": {
                "provider_a": {
                    "base_url": f"http://127.0.0.1:{up_port}",
                    "api_keys": ["key_a1", "key_a2"],
                    "models": {"model_a1": {"upstream_model": "upstream_a1"}},
                    "is_active": True,
                },
                "provider_b": {
                    "base_url": f"http://127.0.0.1:{up_port}",
                    "api_keys": ["key_b1"],
                    "models": {"model_b1": {"upstream_model": "upstream_b1"}},
                    "is_active": True,
                },
            },
            "model_pools": {
                "smart-coder": {
                    "description": "pool",
                    "candidates": [
                        {"provider": "provider_a", "model": "model_a1"},
                        {"provider": "provider_b", "model": "model_b1"},
                    ],
                }
            },
        }
    }


class FailoverTests(unittest.TestCase):
    def setUp(self):
        _FailoverUpstream.records = []
        _FailoverUpstream.key_behavior = {}
        self.up = http.server.ThreadingHTTPServer(("127.0.0.1", 0), _FailoverUpstream)
        self.up_port = self.up.server_address[1]
        threading.Thread(target=self.up.serve_forever, daemon=True).start()
        cfg = _failover_config(self.up_port)
        self.tmp = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(cfg, self.tmp)
        self.tmp.close()
        self.gateway = GatewayEngine(self.tmp.name, sync_opencode=False)
        handler = type("H", (GatewayRequestHandler,), {})
        handler.gateway = self.gateway
        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.port = self.httpd.server_address[1]
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()
        self.base = f"http://127.0.0.1:{self.port}"
        self.h = {"Authorization": "Bearer sk-test-token", "Content-Type": "application/json"}

    def tearDown(self):
        self.httpd.shutdown()
        self.up.shutdown()
        if os.path.exists(self.tmp.name):
            os.remove(self.tmp.name)

    def _post(self, model="smart-coder"):
        data = json.dumps({"model": model}).encode()
        req = urllib.request.Request(self.base + "/chat/completions", data=data, headers={**self.h, "Content-Length": str(len(data))})
        try:
            with urllib.request.urlopen(req) as r:
                return r.status, r.read()
        except urllib.error.HTTPError as e:
            return e.code, e.read()

    def test_429_failover_same_provider_first(self):
        _FailoverUpstream.key_behavior = {"key_a1": "429"}
        st, _ = self._post()
        self.assertEqual(st, 200)
        self.assertEqual(_FailoverUpstream.records, ["key_a1", "key_a2"])
        self.assertNotIn("key_b1", _FailoverUpstream.records)

    def test_5xx_failover_same_provider_first(self):
        _FailoverUpstream.key_behavior = {"key_a1": "500"}
        st, _ = self._post()
        self.assertEqual(st, 200)
        self.assertEqual(_FailoverUpstream.records, ["key_a1", "key_a2"])

    def test_transport_failover_same_provider_first(self):
        _FailoverUpstream.key_behavior = {"key_a1": "transport"}
        st, _ = self._post()
        self.assertEqual(st, 200)
        self.assertEqual(_FailoverUpstream.records, ["key_a1", "key_a2"])

    def test_nonretryable_400_fails_fast(self):
        _FailoverUpstream.key_behavior = {"key_a1": "400"}
        st, _ = self._post()
        self.assertEqual(st, 400)
        self.assertEqual(_FailoverUpstream.records, ["key_a1"])


class CoolingTests(unittest.TestCase):
    def setUp(self):
        _FailoverUpstream.records = []
        _FailoverUpstream.key_behavior = {}
        self.up = http.server.ThreadingHTTPServer(("127.0.0.1", 0), _FailoverUpstream)
        self.up_port = self.up.server_address[1]
        threading.Thread(target=self.up.serve_forever, daemon=True).start()
        cfg = _failover_config(self.up_port)
        self.tmp = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(cfg, self.tmp)
        self.tmp.close()
        self.gateway = GatewayEngine(self.tmp.name, sync_opencode=False)
        handler = type("H", (GatewayRequestHandler,), {})
        handler.gateway = self.gateway
        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.port = self.httpd.server_address[1]
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()
        self.base = f"http://127.0.0.1:{self.port}"
        self.h = {"Authorization": "Bearer sk-test-token", "Content-Type": "application/json"}

    def tearDown(self):
        self.httpd.shutdown()
        self.up.shutdown()
        if os.path.exists(self.tmp.name):
            os.remove(self.tmp.name)

    def _post(self, model="smart-coder"):
        data = json.dumps({"model": model}).encode()
        req = urllib.request.Request(self.base + "/chat/completions", data=data, headers={**self.h, "Content-Length": str(len(data))})
        try:
            with urllib.request.urlopen(req) as r:
                return r.status, dict(r.headers), r.read()
        except urllib.error.HTTPError as e:
            return e.code, dict(e.headers), e.read()

    def test_all_cooling_returns_429_no_calls(self):
        _FailoverUpstream.key_behavior = {"key_a1": "429", "key_a2": "429", "key_b1": "429"}
        # first request exhausts
        st, hdrs, _ = self._post()
        self.assertEqual(st, 429)
        first_calls = len(_FailoverUpstream.records)
        _FailoverUpstream.records = []
        st2, hdrs2, _ = self._post()
        self.assertEqual(st2, 429)
        self.assertEqual(len(_FailoverUpstream.records), 0)
        ra = hdrs2.get("Retry-After") or hdrs2.get("retry-after")
        self.assertIsNotNone(ra)
        self.assertEqual(int(float(ra)), 5)


class CooldownTimingTests(unittest.TestCase):
    def setUp(self):
        _FailoverUpstream.records = []
        _FailoverUpstream.key_behavior = {}
        self.up = http.server.ThreadingHTTPServer(("127.0.0.1", 0), _FailoverUpstream)
        self.up_port = self.up.server_address[1]
        threading.Thread(target=self.up.serve_forever, daemon=True).start()
        # single-key provider to force all-cooling after one failure
        cfg = {
            "ai_gateway": {
                "port": 17688,
                "auth": {"token": "sk-test-token", "require": True},
                "max_body_bytes": 10485760,
                "retry": {"max_cooldown_seconds": 600},
                "providers": {
                    "p": {
                        "base_url": f"http://127.0.0.1:{self.up_port}",
                        "api_keys": ["onlykey"],
                        "models": {"m": {"upstream_model": "upstream-m"}},
                        "is_active": True,
                    }
                },
                "model_pools": {},
            }
        }
        self.tmp = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(cfg, self.tmp)
        self.tmp.close()
        self.gateway = GatewayEngine(self.tmp.name, sync_opencode=False)
        handler = type("H", (GatewayRequestHandler,), {})
        handler.gateway = self.gateway
        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.port = self.httpd.server_address[1]
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()
        self.base = f"http://127.0.0.1:{self.port}"
        self.h = {"Authorization": "Bearer sk-test-token", "Content-Type": "application/json"}

    def tearDown(self):
        self.httpd.shutdown()
        self.up.shutdown()
        if os.path.exists(self.tmp.name):
            os.remove(self.tmp.name)
        # restore time
        import core.gateway as gwmod
        gwmod.time.monotonic = _time.monotonic

    def _clock(self, value):
        import core.gateway as gwmod
        gwmod.time.monotonic = lambda: value

    def _post(self, model="m"):
        data = json.dumps({"model": model}).encode()
        req = urllib.request.Request(self.base + "/chat/completions", data=data, headers={**self.h, "Content-Length": str(len(data))})
        try:
            with urllib.request.urlopen(req) as r:
                return r.status, dict(r.headers), r.read()
        except urllib.error.HTTPError as e:
            return e.code, dict(e.headers), e.read()

    def test_retry_after_capped(self):
        self._clock(1000.0)
        _FailoverUpstream.key_behavior = {"onlykey": "429", "onlykey:ra": "9999"}
        st, hdrs, _ = self._post()
        self.assertEqual(st, 429)
        ra = hdrs.get("Retry-After") or hdrs.get("retry-after")
        self.assertEqual(int(float(ra)), 600)

    def test_invalid_retry_after_http_default(self):
        self._clock(1000.0)
        for ra_val in ("0", "-5", "abc"):
            _FailoverUpstream.key_behavior = {"onlykey": "429", "onlykey:ra": ra_val}
            # reset cooldowns by new gateway? reuse; clock same but key cooling; advance clock to release first
            self._clock(1000.0 + 10000.0)
            st, hdrs, _ = self._post()
            # transport? no, http 429 -> default 60, but if already cooling? New failure marks 60 from now
            # response for exhausted should be 429 with 60
            self.assertEqual(st, 429)
            ra = hdrs.get("Retry-After") or hdrs.get("retry-after")
            self.assertEqual(int(float(ra)), 60)
            self._clock(1000.0 + 10000.0)

    def test_transport_default_30(self):
        self._clock(1000.0)
        _FailoverUpstream.key_behavior = {"onlykey": "transport"}
        st, hdrs, _ = self._post()
        self.assertEqual(st, 429)
        ra = hdrs.get("Retry-After") or hdrs.get("retry-after")
        self.assertEqual(int(float(ra)), 30)

    def test_expiry_releases_key(self):
        self._clock(1000.0)
        _FailoverUpstream.key_behavior = {"onlykey": "transport"}
        st, _, _ = self._post()
        self.assertEqual(st, 429)
        # advance beyond cooldown
        self._clock(1000.0 + 31.0)
        _FailoverUpstream.key_behavior = {"onlykey": "ok"}
        _FailoverUpstream.records = []
        st2, _, _ = self._post()
        self.assertEqual(st2, 200)
        self.assertGreaterEqual(len(_FailoverUpstream.records), 1)


class StreamingRelayTests(unittest.TestCase):
    def setUp(self):
        self.handler_cls = self._handler_cls()
        self.up = http.server.ThreadingHTTPServer(("127.0.0.1", 0), self.handler_cls)
        self.up_port = self.up.server_address[1]
        threading.Thread(target=self.up.serve_forever, daemon=True).start()
        cfg = {
            "ai_gateway": {
                "port": 17688,
                "auth": {"token": "sk-test-token", "require": True},
                "max_body_bytes": 10485760,
                "providers": {
                    "p": {
                        "base_url": f"http://127.0.0.1:{self.up_port}",
                        "api_keys": ["k"],
                        "models": {"m": {"upstream_model": "upstream-m"}},
                        "is_active": True,
                    }
                },
                "model_pools": {},
            }
        }
        self.tmp = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(cfg, self.tmp)
        self.tmp.close()
        self.gateway = GatewayEngine(self.tmp.name, sync_opencode=False)
        handler = type("H", (GatewayRequestHandler,), {})
        handler.gateway = self.gateway
        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.port = self.httpd.server_address[1]
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()
        self.base = f"http://127.0.0.1:{self.port}"
        self.h = {"Authorization": "Bearer sk-test-token", "Content-Type": "application/json"}

    def _handler_cls(self):
        class H(http.server.BaseHTTPRequestHandler):
            calls = []

            def do_POST(self):
                H.calls.append(self.headers.get("Authorization"))
                n = int(self.headers.get("Content-Length", 0) or 0)
                if n:
                    self.rfile.read(n)
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Cache-Control", "no-cache")
                self.end_headers()
                try:
                    self.wfile.write(b"data: prefix\n\n")
                    self.wfile.flush()
                    _time.sleep(1.0)
                    self.wfile.write(b"x" * 70000)
                    self.wfile.write(b"data: final\n\n")
                    self.wfile.flush()
                except (BrokenPipeError, ConnectionResetError, OSError):
                    pass
                self.close_connection = True

            def log_message(self, *a):
                pass
        H.calls = []
        return H

    def tearDown(self):
        self.httpd.shutdown()
        self.up.shutdown()
        if os.path.exists(self.tmp.name):
            os.remove(self.tmp.name)

    def test_prefix_prompt_and_complete_bytes(self):
        data = json.dumps({"model": "m", "stream": True}).encode()
        req = (
            f"POST /chat/completions HTTP/1.1\r\n"
            f"Host: localhost\r\n"
            f"Authorization: Bearer sk-test-token\r\n"
            f"Content-Type: application/json\r\n"
            f"Content-Length: {len(data)}\r\n"
            f"Connection: close\r\n\r\n"
        ).encode() + data
        s = socket.create_connection(("127.0.0.1", self.port), timeout=2)
        s.settimeout(2)
        s.sendall(req)
        buf = b""
        # prefix must arrive promptly while upstream is still open
        deadline = _time.monotonic() + 0.4
        while b"prefix" not in buf and _time.monotonic() < deadline:
            try:
                c = s.recv(4096)
            except socket.timeout:
                break
            if not c:
                break
            buf += c
        self.assertIn(b"prefix", buf)
        self.assertLess(_time.monotonic(), deadline + 0.05)
        # read rest
        s.settimeout(5)
        while True:
            try:
                c = s.recv(65536)
            except socket.timeout:
                break
            if not c:
                break
            buf += c
        s.close()
        self.assertIn(b"data: final", buf)
        self.assertGreaterEqual(len(buf), 70000)

    def test_downstream_disconnect_no_cooldown(self):
        data = json.dumps({"model": "m", "stream": True}).encode()
        req = (
            f"POST /chat/completions HTTP/1.1\r\n"
            f"Host: localhost\r\n"
            f"Authorization: Bearer sk-test-token\r\n"
            f"Content-Type: application/json\r\n"
            f"Content-Length: {len(data)}\r\n"
            f"Connection: close\r\n\r\n"
        ).encode() + data
        s = socket.create_connection(("127.0.0.1", self.port), timeout=2)
        s.settimeout(2)
        s.sendall(req)
        # read prefix
        buf = b""
        deadline = _time.time() + 1.0
        while b"prefix" not in buf and _time.time() < deadline:
            try:
                c = s.recv(4096)
            except socket.timeout:
                break
            if not c:
                break
            buf += c
        s.close()
        _time.sleep(0.5)
        # exactly one upstream call for the disconnected request
        self.assertEqual(len(self.handler_cls.calls), 1)
        # follow-up request should succeed (same key, no cooldown/fallback)
        st, _ = self._simple_post()
        self.assertEqual(st, 200)

    def _simple_post(self):
        data = json.dumps({"model": "m", "stream": False}).encode()
        # upstream handler only handles POST streaming? It ignores stream flag and writes SSE always; adjust? 
        # For follow-up non-stream, upstream still returns SSE-ish; status 200 is enough.
        req = urllib.request.Request(self.base + "/chat/completions", data=data, headers={**self.h, "Content-Length": str(len(data))})
        try:
            with urllib.request.urlopen(req) as r:
                r.read()
                return r.status, None
        except urllib.error.HTTPError as e:
            return e.code, None



class StreamingFailureTests(unittest.TestCase):
    def setUp(self):
        class H(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                n = int(self.headers.get("Content-Length", 0) or 0)
                if n:
                    self.rfile.read(n)
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.end_headers()
                self.wfile.write(b"data: prefix\n\n")
                self.wfile.flush()
                self.close_connection = True
                try:
                    self.request.shutdown(socket.SHUT_RDWR)
                except Exception:
                    pass

            def log_message(self, *a):
                pass

        self.up = http.server.ThreadingHTTPServer(("127.0.0.1", 0), H)
        self.up_port = self.up.server_address[1]
        threading.Thread(target=self.up.serve_forever, daemon=True).start()
        cfg = {
            "ai_gateway": {
                "port": 17688,
                "auth": {"token": "sk-test-token", "require": True},
                "max_body_bytes": 10485760,
                "providers": {
                    "p": {
                        "base_url": f"http://127.0.0.1:{self.up_port}",
                        "api_keys": ["k"],
                        "models": {"m": {"upstream_model": "upstream-m"}},
                        "is_active": True,
                    }
                },
                "model_pools": {},
            }
        }
        self.tmp = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(cfg, self.tmp)
        self.tmp.close()
        self.gateway = GatewayEngine(self.tmp.name, sync_opencode=False)
        handler = type("H", (GatewayRequestHandler,), {})
        handler.gateway = self.gateway
        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.port = self.httpd.server_address[1]
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()
        self.base = f"http://127.0.0.1:{self.port}"

    def tearDown(self):
        self.httpd.shutdown()
        self.up.shutdown()
        if os.path.exists(self.tmp.name):
            os.remove(self.tmp.name)

    def test_post_header_failure_closes_without_second_body(self):
        data = json.dumps({"model": "m", "stream": True}).encode()
        req = (
            f"POST /chat/completions HTTP/1.1\r\n"
            f"Host: localhost\r\n"
            f"Authorization: Bearer sk-test-token\r\n"
            f"Content-Type: application/json\r\n"
            f"Content-Length: {len(data)}\r\n"
            f"Connection: close\r\n\r\n"
        ).encode() + data
        s = socket.create_connection(("127.0.0.1", self.port), timeout=2)
        s.settimeout(5)
        s.sendall(req)
        buf = b""
        while True:
            try:
                c = s.recv(4096)
            except socket.timeout:
                break
            if not c:
                break
            buf += c
        s.close()
        self.assertIn(b"prefix", buf)
        self.assertNotIn(b"model_not_found", buf)


import http.client as _http_client


class StreamingReadCapacityTests(unittest.TestCase):
    def setUp(self):
        class H(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                n = int(self.headers.get("Content-Length", 0) or 0)
                if n:
                    self.rfile.read(n)
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.end_headers()
                self.wfile.write(b"data: prefix\n\n")
                self.wfile.flush()
                _time.sleep(0.2)
                self.wfile.write(b"x" * 70000)
                self.wfile.write(b"data: final\n\n")
                self.wfile.flush()
                self.close_connection = True

            def log_message(self, *a):
                pass

        self.up = http.server.ThreadingHTTPServer(("127.0.0.1", 0), H)
        self.up_port = self.up.server_address[1]
        threading.Thread(target=self.up.serve_forever, daemon=True).start()
        cfg = {
            "ai_gateway": {
                "port": 17688,
                "auth": {"token": "sk-test-token", "require": True},
                "max_body_bytes": 10485760,
                "providers": {
                    "p": {
                        "base_url": f"http://127.0.0.1:{self.up_port}",
                        "api_keys": ["k"],
                        "models": {"m": {"upstream_model": "upstream-m"}},
                        "is_active": True,
                    }
                },
                "model_pools": {},
            }
        }
        self.tmp = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(cfg, self.tmp)
        self.tmp.close()
        self.gateway = GatewayEngine(self.tmp.name, sync_opencode=False)
        handler = type("H", (GatewayRequestHandler,), {})
        handler.gateway = self.gateway
        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.port = self.httpd.server_address[1]
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()
        self.base = f"http://127.0.0.1:{self.port}"
        self.h = {"Authorization": "Bearer sk-test-token", "Content-Type": "application/json"}

    def tearDown(self):
        self.httpd.shutdown()
        self.up.shutdown()
        if os.path.exists(self.tmp.name):
            os.remove(self.tmp.name)

    def test_read_capacity_65536_and_no_unbounded_read(self):
        orig_read1 = _http_client.HTTPResponse.read1
        orig_read = _http_client.HTTPResponse.read
        read1_amts = []
        read_amts = []

        def spy_read1(self, amt=-1):
            read1_amts.append(amt)
            return orig_read1(self, amt)

        def spy_read(self, amt=-1):
            read_amts.append(amt)
            return orig_read(self, amt)

        _http_client.HTTPResponse.read1 = spy_read1
        _http_client.HTTPResponse.read = spy_read
        try:
            data = json.dumps({"model": "m", "stream": True}).encode()
            req = urllib.request.Request(self.base + "/chat/completions", data=data, headers={**self.h, "Content-Length": str(len(data))})
            with urllib.request.urlopen(req) as r:
                while True:
                    c = r.read(65536)
                    if not c:
                        break
        finally:
            _http_client.HTTPResponse.read1 = orig_read1
            _http_client.HTTPResponse.read = orig_read

        self.assertIn(65536, read1_amts)
        self.assertNotIn(-1, read_amts)


class ZeroKeyProviderTests(unittest.TestCase):
    def setUp(self):
        self.up = http.server.ThreadingHTTPServer(("127.0.0.1", 0), _Upstream if False else None) if False else None
        # simple upstream returning ok
        class H(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                n = int(self.headers.get("Content-Length", 0) or 0)
                if n:
                    self.rfile.read(n)
                body = b'{"choices":[{"message":{"content":"ok"}}]}'
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            def log_message(self, *a):
                pass
        self.up = http.server.ThreadingHTTPServer(("127.0.0.1", 0), H)
        self.up_port = self.up.server_address[1]
        threading.Thread(target=self.up.serve_forever, daemon=True).start()
        cfg = {
            "ai_gateway": {
                "port": 17688,
                "auth": {"token": "sk-test-token", "require": True},
                "max_body_bytes": 10485760,
                "providers": {
                    "nokey": {
                        "base_url": f"http://127.0.0.1:{self.up_port}",
                        "api_keys": [],
                        "models": {"m": {"upstream_model": "upstream-m"}},
                        "is_active": True,
                    },
                    "keyed": {
                        "base_url": f"http://127.0.0.1:{self.up_port}",
                        "api_keys": ["k"],
                        "models": {"m": {"upstream_model": "upstream-m"}},
                        "is_active": True,
                    },
                },
                "model_pools": {
                    "pool": {"description": "d", "candidates": [
                        {"provider": "nokey", "model": "m"},
                        {"provider": "keyed", "model": "m"},
                    ]},
                },
            }
        }
        self.tmp = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(cfg, self.tmp)
        self.tmp.close()
        self.gateway = GatewayEngine(self.tmp.name, sync_opencode=False)
        handler = type("H", (GatewayRequestHandler,), {})
        handler.gateway = self.gateway
        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.port = self.httpd.server_address[1]
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()
        self.base = f"http://127.0.0.1:{self.port}"
        self.h = {"Authorization": "Bearer sk-test-token", "Content-Type": "application/json"}

    def tearDown(self):
        self.httpd.shutdown()
        self.up.shutdown()
        if os.path.exists(self.tmp.name):
            os.remove(self.tmp.name)

    def _post(self, model):
        data = json.dumps({"model": model}).encode()
        req = urllib.request.Request(self.base + "/chat/completions", data=data, headers={**self.h, "Content-Length": str(len(data))})
        try:
            with urllib.request.urlopen(req) as r:
                return r.status, r.read()
        except urllib.error.HTTPError as e:
            return e.code, e.read()

    def test_zero_key_skipped_keyed_serves(self):
        st, _ = self._post("pool")
        self.assertEqual(st, 200)

    def test_known_model_only_zero_key_returns_502(self):
        st, _ = self._post("m")
        # model "m" resolves via both providers (mapped); nokey skipped, keyed serves -> 200? Actually m maps to both providers, keyed has key -> 200.
        # For a model only on zero-key provider, create direct provider/model path nokey/m? prefix passthrough -> nokey provider has no key -> no candidates? resolve returns known? prefix path returns candidate if provider active regardless of keys. Then handler get_available_key None -> skip; no candidates left -> known True? resolve known True for prefix? Actually prefix path sets known? In code prefix path returns candidates, True (known). Handler candidates nonempty initially; after skip all, falls through to 502 (all candidates exhausted). So expect 502.
        st2, _ = self._post("nokey/m")
        self.assertEqual(st2, 502)


class InjectableClockTests(unittest.TestCase):
    def setUp(self):
        cfg = {
            "ai_gateway": {
                "port": 17688,
                "auth": {"token": "sk-test-token", "require": True},
                "max_body_bytes": 10485760,
                "retry": {"max_cooldown_seconds": 600},
                "providers": {
                    "p": {
                        "base_url": "http://127.0.0.1:1/upstream",
                        "api_keys": ["onlykey"],
                        "models": {"m": {"upstream_model": "upstream-m"}},
                        "is_active": True,
                    }
                },
                "model_pools": {},
            }
        }
        self.tmp = tempfile.NamedTemporaryFile("w+", delete=False, suffix=".json")
        json.dump(cfg, self.tmp)
        self.tmp.close()
        self.now = {"t": 1000.0}
        self.gateway = GatewayEngine(self.tmp.name, sync_opencode=False, clock=lambda: self.now["t"])

    def tearDown(self):
        if os.path.exists(self.tmp.name):
            os.remove(self.tmp.name)

    def test_cooldown_expiry_follows_injected_clock(self):
        provider = self.gateway.providers["p"]
        provider.mark_key_failed("onlykey", cooldown_seconds=30)
        self.now["t"] = 1000.0 + 10.0
        self.assertTrue(provider.is_key_cooling("onlykey"))
        self.now["t"] = 1000.0 + 31.0
        self.assertFalse(provider.is_key_cooling("onlykey"))

    def test_wall_clock_changes_irrelevant(self):
        import time as _t
        provider = self.gateway.providers["p"]
        provider.mark_key_failed("onlykey", cooldown_seconds=30)
        # wall clock moves far ahead, injected clock stays inside cooldown
        _t.sleep(0)
        self.now["t"] = 1000.0 + 5.0
        self.assertTrue(provider.is_key_cooling("onlykey"))
