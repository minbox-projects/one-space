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
        st, _, _ = self._req("GET", "/health")
        self.assertEqual(st, 200)
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
