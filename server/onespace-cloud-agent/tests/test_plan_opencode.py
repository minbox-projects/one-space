#!/usr/bin/env python3
"""Behavior tests for core/plan_opencode.py (RED until module exists)."""

import json
import threading
import unittest
import urllib.parse
import urllib.request

from core.plan_opencode import (
    RECOMMENDED_MARKERS,
    OpenCodeError,
    OpenCodeClient,
    recommended_answers,
    owned_session_ids,
)


def _q(multiple=False, options=None):
    opts = [{"label": o} for o in (options or [])]
    return {"question": "Q?", "multiple": multiple, "options": opts}


class TestRecommendedAnswers(unittest.TestCase):
    def test_en(self):
        req = {"questions": [_q(options=["X (recommended)", "Y"])]}
        self.assertEqual(recommended_answers(req), [["X (recommended)"]])

    def test_zh(self):
        req = {"questions": [_q(options=["X (推荐)", "Y"])]}
        self.assertEqual(recommended_answers(req), [["X (推荐)"]])

    def test_fullwidth(self):
        req = {"questions": [_q(options=["X （推荐）", "Y"])]}
        self.assertEqual(recommended_answers(req), [["X （推荐）"]])

    def test_whitespace_case(self):
        label = "  X  (RECOMMENDED)  "
        req = {"questions": [_q(options=[label, "Y"])]}
        self.assertEqual(recommended_answers(req), [[label]])

    def test_two_markers_none(self):
        req = {"questions": [_q(options=["X (recommended)", "Y (recommended)"])]}
        self.assertIsNone(recommended_answers(req))

    def test_multiple_choice_none(self):
        req = {"questions": [_q(multiple=True, options=["X (recommended)", "Y"])]}
        self.assertIsNone(recommended_answers(req))

    def test_no_options_none(self):
        req = {"questions": [_q(options=[])]}
        self.assertIsNone(recommended_answers(req))

    def test_no_marker_none(self):
        req = {"questions": [_q(options=["X", "Y"])]}
        self.assertIsNone(recommended_answers(req))

    def test_original_label_preserved(self):
        label = "  spaced label (recommended)  "
        req = {"questions": [_q(options=[label, "Y"])]}
        self.assertEqual(recommended_answers(req), [[label]])

    def test_scope_prose_ignored(self):
        req = {"questions": [{"question": "Q?", "multiple": False,
                               "options": [{"label": "X", "description": "covers scope and acceptance criteria"}]}]}
        self.assertIsNone(recommended_answers(req))

    def test_multi_all_single(self):
        req = {"questions": [
            _q(options=["X (recommended)", "Y"]),
            _q(options=["P", "Q (推荐)"]),
        ]}
        self.assertEqual(recommended_answers(req), [["X (recommended)"], ["Q (推荐)"]])

    def test_multi_one_fails(self):
        req = {"questions": [
            _q(options=["X (recommended)", "Y"]),
            _q(options=["P", "Q"]),
        ]}
        self.assertIsNone(recommended_answers(req))

    def test_empty_none(self):
        self.assertIsNone(recommended_answers({}))
        self.assertIsNone(recommended_answers({"questions": []}))


class _Resp:
    def __init__(self, body):
        self._body = body.encode() if isinstance(body, str) else body
    def read(self):
        return self._body
    def __enter__(self):
        return self
    def __exit__(self, *a):
        return False


class FakeOpener:
    def __init__(self, body='{}'):
        self.body = body
        self.calls = []
    def __call__(self, req, timeout=None):
        self.calls.append((req, timeout))
        return _Resp(self.body)


class _ChunkedResp:
    """Response whose ``read`` returns one queued chunk per call."""

    def __init__(self, chunks):
        self._chunks = list(chunks)

    def read(self, _size=-1):
        if not self._chunks:
            return b""
        return self._chunks.pop(0)

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False


class ChunkedOpener:
    def __init__(self, chunks):
        self.chunks = list(chunks)
        self.calls = []

    def __call__(self, req, timeout=None):
        self.calls.append((req, timeout))
        return _ChunkedResp(self.chunks)


class TestOpenCodeClient(unittest.TestCase):
    BASE = "http://127.0.0.1:4096"
    DIR = "/cloud/root"

    def _client(self, body='{}'):
        fo = FakeOpener(body)
        c = OpenCodeClient(self.BASE, self.DIR, opener=fo, timeout=5.0)
        return c, fo

    def _url(self, fo):
        return fo.calls[0][0].full_url

    def test_doc_get(self):
        c, fo = self._client('{"ok":true}')
        out = c.doc()
        self.assertEqual(out, {"ok": True})
        req = fo.calls[0][0]
        self.assertEqual(req.get_method(), "GET")
        self.assertIn("/doc", req.full_url)
        self.assertIn("directory=", req.full_url)

    def test_create_session_post_body(self):
        c, fo = self._client('{"id":"s1"}')
        out = c.create_session(title="t", parent_id="p")
        self.assertEqual(out, {"id": "s1"})
        req = fo.calls[0][0]
        self.assertEqual(req.get_method(), "POST")
        self.assertIn("/session", req.full_url)
        self.assertIn("directory=", req.full_url)
        data = json.loads(req.data.decode())
        self.assertEqual(data.get("title"), "t")
        self.assertEqual(data.get("parentID"), "p")

    def test_prompt_async(self):
        c, fo = self._client('')
        c.prompt_async("sid", "hello", agent="a", model="m")
        req = fo.calls[0][0]
        self.assertEqual(req.get_method(), "POST")
        self.assertIn("/session/sid/prompt_async", req.full_url)
        self.assertIn("directory=", req.full_url)
        data = json.loads(req.data.decode())
        self.assertEqual(data.get("agent"), "a")

    def test_children(self):
        c, fo = self._client('[]')
        out = c.children("sid")
        self.assertEqual(out, [])
        req = fo.calls[0][0]
        self.assertEqual(req.get_method(), "GET")
        self.assertIn("/session/sid/children", req.full_url)

    def test_session_status(self):
        c, fo = self._client('{}')
        c.session_status()
        req = fo.calls[0][0]
        self.assertEqual(req.get_method(), "GET")
        self.assertIn("/session/status", req.full_url)

    def test_permissions(self):
        c, fo = self._client('[]')
        c.permissions()
        req = fo.calls[0][0]
        self.assertEqual(req.get_method(), "GET")
        self.assertIn("/permission", req.full_url)

    def test_reply_permission_exact(self):
        c, fo = self._client('{}')
        c.reply_permission("rid", reply="once")
        req = fo.calls[0][0]
        self.assertEqual(req.get_method(), "POST")
        self.assertIn("/permission/rid/reply", req.full_url)
        data = json.loads(req.data.decode())
        self.assertEqual(data, {"reply": "once"})

    def test_questions(self):
        c, fo = self._client('[]')
        c.questions()
        req = fo.calls[0][0]
        self.assertEqual(req.get_method(), "GET")
        self.assertIn("/question", req.full_url)

    def test_reply_question_exact(self):
        c, fo = self._client('{}')
        c.reply_question("rid", [["a"]])
        req = fo.calls[0][0]
        self.assertEqual(req.get_method(), "POST")
        self.assertIn("/question/rid/reply", req.full_url)
        data = json.loads(req.data.decode())
        self.assertEqual(data, {"answers": [["a"]]})

    def test_stream_events_directory(self):
        body = "data: {\"type\":\"x\"}\n\ndata: {\"type\":\"y\"}\n\n"
        fo = FakeOpener(body)
        c = OpenCodeClient(self.BASE, self.DIR, opener=fo, timeout=5.0)
        events = []
        stop = threading.Event()
        c.stream_events(stop, events.append)
        self.assertEqual(events, [{"type": "x"}, {"type": "y"}])
        req = fo.calls[0][0]
        self.assertIn("/event", req.full_url)
        self.assertIn("directory=", req.full_url)

    def test_split_chunks_sse(self):
        chunks = [b'data: {"type":"spl', b'it"}\n\n', b""]
        fo = ChunkedOpener(chunks)
        c = OpenCodeClient(self.BASE, self.DIR, opener=fo, timeout=5.0)
        events = []
        stop = threading.Event()
        c.stream_events(stop, events.append)
        self.assertEqual(events, [{"type": "split"}])
        req = fo.calls[0][0]
        self.assertIn("/event", req.full_url)
        self.assertIn("directory=", req.full_url)


class TestOwnedSessionIds(unittest.TestCase):
    def test_parent_and_descendants(self):
        sessions = [
            {"id": "p", "parentID": None},
            {"id": "c1", "parentID": "p"},
            {"id": "c2", "parentID": "c1"},
            {"id": "foreign", "parentID": "other"},
        ]
        self.assertEqual(owned_session_ids("p", sessions), {"p", "c1", "c2"})

    def test_foreign_excluded(self):
        sessions = [{"id": "p"}, {"id": "f", "parentID": "x"}]
        self.assertEqual(owned_session_ids("p", sessions), {"p"})


if __name__ == "__main__":
    unittest.main()
