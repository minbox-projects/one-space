#!/usr/bin/env python3
"""Directory-scoped OpenCode v1 client used by cloud plan execution.

The cloud controller talks to one job-owned loopback ``opencode serve``
process through the published v1 session, permission and question routes.
Every request, including event streaming, carries the same project directory
scope so pending and reply calls cannot leak across sessions.
"""

import json
import socket
import urllib.error
import urllib.parse
import urllib.request

RECOMMENDED_MARKERS = ("(recommended)", "(推荐)", "（推荐）")


class OpenCodeError(Exception):
    """A handled OpenCode transport or protocol failure."""

    def __init__(self, code, message):
        super().__init__(message)
        self.code = code
        self.message = message


def normalize_label(label):
    """Return the surrounding-whitespace-normalized option label."""
    if not isinstance(label, str):
        return ""
    return label.strip()


def _has_marker(label):
    normalized = normalize_label(label)
    if not normalized:
        return False
    lowered = normalized.lower()
    if lowered.endswith(RECOMMENDED_MARKERS[0]):
        return True
    return normalized.endswith(RECOMMENDED_MARKERS[1]) or normalized.endswith(
        RECOMMENDED_MARKERS[2]
    )


def recommended_answers(request):
    """Return original labels when every question has exactly one marker.

    The rule is purely syntactic: a question must be single-choice, carry at
    least one option and have exactly one option whose normalized label ends
    with a recommended marker.  Any other shape returns ``None``.
    """
    if not isinstance(request, dict):
        return None
    questions = request.get("questions")
    if not isinstance(questions, list) or not questions:
        return None

    answers = []
    for question in questions:
        if not isinstance(question, dict):
            return None
        if question.get("multiple") is True:
            return None
        options = question.get("options")
        if not isinstance(options, list) or not options:
            return None
        marked = []
        for option in options:
            if not isinstance(option, dict):
                return None
            label = option.get("label")
            if not isinstance(label, str) or not label.strip():
                return None
            if _has_marker(label):
                marked.append(label)
        if len(marked) != 1:
            return None
        answers.append([marked[0]])
    return answers


def owned_session_ids(parent_id, sessions):
    """Return the parent and every transitive descendant session id."""
    owned = {parent_id}
    if not isinstance(sessions, list):
        return owned
    children = {}
    for session in sessions:
        if not isinstance(session, dict):
            continue
        session_id = session.get("id")
        parent = session.get("parentID")
        if isinstance(session_id, str) and isinstance(parent, str):
            children.setdefault(parent, []).append(session_id)
    pending = [parent_id]
    while pending:
        current = pending.pop()
        for child in children.get(current, []):
            if child not in owned:
                owned.add(child)
                pending.append(child)
    return owned


class OpenCodeClient:
    """Minimal v1 client with a stable ``?directory=`` project scope."""

    def __init__(self, base_url, directory, *, opener=None, timeout=30.0):
        self.base_url = str(base_url).rstrip("/")
        self.directory = directory
        self.timeout = timeout
        self._opener = opener

    # ------------------------------------------------------------------
    # Transport
    # ------------------------------------------------------------------
    def _url(self, path):
        query = urllib.parse.urlencode({"directory": self.directory})
        return "%s%s?%s" % (self.base_url, path, query)

    def _request(self, method, path, payload=None):
        body = None
        headers = {}
        if payload is not None:
            body = json.dumps(payload).encode("utf-8")
            headers["Content-Type"] = "application/json"
        request = urllib.request.Request(
            self._url(path), data=body, headers=headers, method=method
        )
        opener = self._opener or urllib.request.urlopen
        try:
            response = opener(request, timeout=self.timeout)
        except (urllib.error.URLError, OSError) as exc:
            raise OpenCodeError("transport", "OpenCode request failed: %s" % exc)
        try:
            with response:
                raw = response.read()
        except OSError as exc:
            raise OpenCodeError("transport", "OpenCode response failed: %s" % exc)
        if not raw:
            return {}
        try:
            return json.loads(raw.decode("utf-8"))
        except (UnicodeDecodeError, ValueError) as exc:
            raise OpenCodeError("protocol", "invalid OpenCode JSON response: %s" % exc)

    def _get(self, path):
        return self._request("GET", path)

    def _post(self, path, payload):
        return self._request("POST", path, payload)

    # ------------------------------------------------------------------
    # Capabilities and sessions
    # ------------------------------------------------------------------
    def doc(self):
        return self._get("/doc")

    def create_session(self, title=None, parent_id=None):
        payload = {}
        if title is not None:
            payload["title"] = title
        if parent_id is not None:
            payload["parentID"] = parent_id
        return self._post("/session", payload)

    def prompt_async(self, session_id, text, *, agent=None, model=None):
        payload = {"parts": [{"type": "text", "text": text}]}
        if agent is not None:
            payload["agent"] = agent
        if model is not None:
            payload["model"] = model
        quoted = urllib.parse.quote(str(session_id), safe="")
        return self._post("/session/%s/prompt_async" % quoted, payload)

    def children(self, session_id):
        quoted = urllib.parse.quote(str(session_id), safe="")
        return self._get("/session/%s/children" % quoted)

    def session_status(self):
        return self._get("/session/status")

    # ------------------------------------------------------------------
    # Pending permissions and questions
    # ------------------------------------------------------------------
    def permissions(self):
        return self._get("/permission")

    def reply_permission(self, request_id, reply="once"):
        quoted = urllib.parse.quote(str(request_id), safe="")
        return self._post("/permission/%s/reply" % quoted, {"reply": reply})

    def questions(self):
        return self._get("/question")

    def reply_question(self, request_id, answers):
        quoted = urllib.parse.quote(str(request_id), safe="")
        return self._post("/question/%s/reply" % quoted, {"answers": answers})

    # ------------------------------------------------------------------
    # Events (SSE)
    # ------------------------------------------------------------------
    def stream_events(self, stop_event, on_event):
        request = urllib.request.Request(self._url("/event"), method="GET")
        if self._opener is not None:
            response = self._opener(request, timeout=self.timeout)
            self._stream_response(response, stop_event, on_event, chunked=False)
            return
        try:
            response = urllib.request.urlopen(
                request, timeout=min(float(self.timeout), 1.0)
            )
        except (urllib.error.URLError, OSError) as exc:
            raise OpenCodeError("transport", "OpenCode event stream failed: %s" % exc)
        try:
            socket_handle = getattr(
                getattr(getattr(response, "fp", None), "raw", None), "_sock", None
            )
            if socket_handle is not None:
                socket_handle.settimeout(min(float(self.timeout), 0.5))
        except OSError:
            pass
        try:
            self._stream_response(response, stop_event, on_event, chunked=True)
        finally:
            try:
                response.close()
            except OSError:
                pass

    @staticmethod
    def _stream_response(response, stop_event, on_event, *, chunked):
        buffer = ""
        data_lines = []
        last_chunk = None
        try:
            with response:
                while True:
                    if stop_event is not None and stop_event.is_set():
                        break
                    try:
                        chunk = response.read(4096) if chunked else response.read()
                    except (socket.timeout, TimeoutError):
                        if stop_event is not None and stop_event.is_set():
                            break
                        continue
                    except OSError as exc:
                        if stop_event is not None and stop_event.is_set():
                            break
                        raise OpenCodeError(
                            "transport", "OpenCode event stream failed: %s" % exc
                        )
                    if not chunk:
                        break
                    if not chunked and chunk == last_chunk:
                        # Non-streaming test doubles repeat their whole body;
                        # one complete delivery must not be replayed forever.
                        break
                    last_chunk = chunk
                    if isinstance(chunk, bytes):
                        buffer += chunk.decode("utf-8", "replace")
                    else:
                        buffer += str(chunk)
                    buffer, data_lines = OpenCodeClient._consume_events(
                        buffer, data_lines, on_event
                    )
        finally:
            if data_lines:
                OpenCodeClient._emit(data_lines, on_event)

    @staticmethod
    def _consume_events(buffer, data_lines, on_event):
        while "\n" in buffer:
            line, buffer = buffer.split("\n", 1)
            if line.endswith("\r"):
                line = line[:-1]
            if line == "":
                if data_lines:
                    OpenCodeClient._emit(data_lines, on_event)
                    data_lines = []
                continue
            if line.startswith(":"):
                continue
            if line.startswith("data:"):
                value = line[len("data:"):]
                if value.startswith(" "):
                    value = value[1:]
                data_lines.append(value)
        return buffer, data_lines

    @staticmethod
    def _emit(data_lines, on_event):
        raw = "\n".join(data_lines)
        try:
            event = json.loads(raw)
        except ValueError:
            return
        on_event(event)
