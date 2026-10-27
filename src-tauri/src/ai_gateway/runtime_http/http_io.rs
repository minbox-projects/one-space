use super::*;

#[derive(Debug)]
pub(in crate::ai_gateway) struct HttpRequest {
    pub(in crate::ai_gateway) method: String,
    pub(in crate::ai_gateway) path: String,
    pub(in crate::ai_gateway) headers: HashMap<String, String>,
    pub(in crate::ai_gateway) body: Vec<u8>,
}

pub(in crate::ai_gateway) struct HttpResponse {
    pub(in crate::ai_gateway) status: u16,
    pub(in crate::ai_gateway) content_type: &'static str,
    pub(in crate::ai_gateway) body: Vec<u8>,
    /// Usage/provider metadata captured while forwarding, consumed by the
    /// request logger. Never forwarded to the caller.
    pub(in crate::ai_gateway) capture: Option<ForwardCapture>,
}

pub(in crate::ai_gateway) async fn read_http_request(
    stream: &mut TcpStream,
) -> Result<HttpRequest, String> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let mut header_end = None;
    loop {
        let read = stream.read(&mut tmp).await.map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..read]);
        if let Some(pos) = find_header_end(&buf) {
            header_end = Some(pos);
            break;
        }
        if buf.len() > 1024 * 1024 {
            return Err("request headers too large".to_string());
        }
    }
    let header_end = header_end.ok_or_else(|| "invalid http request".to_string())?;
    let headers_raw = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = headers_raw.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| "missing request line".to_string())?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or("").to_string();
    let path = request_parts.next().unwrap_or("").to_string();
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = buf[(header_end + 4)..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut tmp).await.map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..read]);
    }
    body.truncate(content_length);
    Ok(HttpRequest {
        method,
        path,
        headers,
        body,
    })
}

pub(in crate::ai_gateway) fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|window| window == b"\r\n\r\n")
}

pub(in crate::ai_gateway) fn json_response(status: u16, body: Value) -> HttpResponse {
    let payload = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
    HttpResponse {
        status,
        content_type: "application/json",
        body: payload,
        capture: None,
    }
}

pub(in crate::ai_gateway) fn reason_for_status(status: u16) -> &'static str {
    match status {
        200..=299 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        502 => "Bad Gateway",
        _ => "Internal Server Error",
    }
}

pub(in crate::ai_gateway) fn http_response_bytes(response: HttpResponse) -> Vec<u8> {
    let header = format!(
        "HTTP/1.1 {} {}\r\ncontent-type: {}\r\naccess-control-allow-origin: *\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        response.status,
        reason_for_status(response.status),
        response.content_type,
        response.body.len()
    );
    [header.into_bytes(), response.body].concat()
}

pub(super) async fn write_response<W: AsyncWrite + Unpin>(
    writer: &mut W,
    response: HttpResponse,
) -> Result<(), String> {
    writer
        .write_all(&http_response_bytes(response))
        .await
        .map_err(|e| e.to_string())
}

/// Add HTTP headers only when a native Messages attempt actually starts an SSE
/// body. Pre-first-byte JSON responses already contain their own HTTP headers,
/// while the direct streaming seam intentionally exposes only native SSE bytes.
pub(super) struct NativeHttpWriter<W> {
    inner: W,
    decided: bool,
    pending: Vec<u8>,
}

impl<W> NativeHttpWriter<W> {
    pub(super) fn new(inner: W) -> Self {
        Self {
            inner,
            decided: false,
            pending: Vec::new(),
        }
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for NativeHttpWriter<W> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        if !self.decided {
            self.decided = true;
            if !buf.starts_with(b"HTTP/1.1 ") {
                self.pending = b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\naccess-control-allow-origin: *\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n".to_vec();
            }
        }
        while !self.pending.is_empty() {
            let pending = std::mem::take(&mut self.pending);
            match std::pin::Pin::new(&mut self.inner).poll_write(cx, &pending) {
                std::task::Poll::Ready(Ok(written)) => {
                    if written < pending.len() {
                        self.pending = pending[written..].to_vec();
                        return std::task::Poll::Pending;
                    }
                }
                std::task::Poll::Ready(Err(error)) => return std::task::Poll::Ready(Err(error)),
                std::task::Poll::Pending => {
                    self.pending = pending;
                    return std::task::Poll::Pending;
                }
            }
        }
        std::pin::Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

pub(super) async fn write_stream_headers<W: AsyncWrite + Unpin>(
    writer: &mut W,
    status: u16,
) -> Result<(), String> {
    let header = format!(
        "HTTP/1.1 {} {}\r\ncontent-type: text/event-stream\r\naccess-control-allow-origin: *\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n",
        status,
        reason_for_status(status)
    );
    writer
        .write_all(header.as_bytes())
        .await
        .map_err(|e| e.to_string())
}

