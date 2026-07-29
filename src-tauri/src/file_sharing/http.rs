use super::runtime::{
    begin_transfer, finish_transfer, update_transfer_progress, SharedFile, SharedSession,
};
use super::types::FileSharingTransferState;
use bytes::Bytes;
use futures_util::Stream;
use http_body_util::{combinators::BoxBody, BodyExt, Full, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::{Method, Request, Response, StatusCode};
use std::convert::Infallible;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
use tokio_util::{
    io::ReaderStream,
    sync::{CancellationToken, WaitForCancellationFutureOwned},
};

type ResponseBody = BoxBody<Bytes, io::Error>;

pub(crate) struct DownloadStream {
    reader: ReaderStream<tokio::io::Take<tokio::fs::File>>,
    cancellation: Pin<Box<WaitForCancellationFutureOwned>>,
    cancellation_token: CancellationToken,
    session: SharedSession,
    transfer_id: String,
    bytes_sent: u64,
    finished: bool,
}

impl DownloadStream {
    pub(crate) fn new(
        file: tokio::fs::File,
        response_bytes: u64,
        session: SharedSession,
        transfer_id: String,
    ) -> Self {
        let cancellation_token = session.cancellation.clone();
        Self {
            reader: ReaderStream::new(file.take(response_bytes)),
            cancellation: Box::pin(cancellation_token.clone().cancelled_owned()),
            cancellation_token,
            session,
            transfer_id,
            bytes_sent: 0,
            finished: false,
        }
    }

    fn finish(&mut self, state: FileSharingTransferState, error: Option<String>) {
        if self.finished {
            return;
        }
        self.finished = true;
        finish_transfer(
            &self.session,
            &self.transfer_id,
            state,
            self.bytes_sent,
            error,
        );
    }
}

impl Unpin for DownloadStream {}

impl Stream for DownloadStream {
    type Item = Result<Frame<Bytes>, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.finished {
            return Poll::Ready(None);
        }
        if self.cancellation.as_mut().poll(context).is_ready() {
            self.finish(FileSharingTransferState::Cancelled, None);
            return Poll::Ready(None);
        }
        match Pin::new(&mut self.reader).poll_next(context) {
            Poll::Ready(Some(Ok(chunk))) => {
                self.bytes_sent += chunk.len() as u64;
                update_transfer_progress(&self.session, &self.transfer_id, self.bytes_sent);
                Poll::Ready(Some(Ok(Frame::data(chunk))))
            }
            Poll::Ready(Some(Err(error))) => {
                self.finish(FileSharingTransferState::Failed, Some(error.to_string()));
                Poll::Ready(Some(Err(error)))
            }
            Poll::Ready(None) => {
                self.finish(FileSharingTransferState::Completed, None);
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for DownloadStream {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let state = if self.cancellation_token.is_cancelled() {
            FileSharingTransferState::Cancelled
        } else {
            FileSharingTransferState::ClientDisconnected
        };
        self.finish(state, None);
    }
}

fn empty(status: StatusCode) -> Response<ResponseBody> {
    Response::builder()
        .status(status)
        .body(
            Full::new(Bytes::new())
                .map_err(|never: Infallible| match never {})
                .boxed(),
        )
        .expect("valid empty response")
}

fn security_headers(builder: hyper::http::response::Builder) -> hyper::http::response::Builder {
    builder
        .header("Cache-Control", "no-store")
        .header("Referrer-Policy", "no-referrer")
        .header("X-Content-Type-Options", "nosniff")
}

fn html_escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '&' => "&amp;".chars().collect::<Vec<_>>(),
            '<' => "&lt;".chars().collect(),
            '>' => "&gt;".chars().collect(),
            '"' => "&quot;".chars().collect(),
            '\'' => "&#39;".chars().collect(),
            _ => vec![character],
        })
        .collect()
}

pub(crate) fn safe_header_filename(name: &str) -> String {
    name.chars()
        .filter(|character| !character.is_control() && *character != '\r' && *character != '\n')
        .map(|character| if character.is_ascii() { character } else { '_' })
        .collect::<String>()
        .replace('"', "'")
}

fn utf8_filename(name: &str) -> String {
    let bytes = name
        .as_bytes()
        .iter()
        .copied()
        .filter(|byte| *byte >= 0x20 && *byte != b'\r' && *byte != b'\n')
        .collect::<Vec<_>>();
    url::form_urlencoded::byte_serialize(&bytes).collect()
}

fn format_size(size: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = size as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{size} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn listing(session: &SharedSession) -> Response<ResponseBody> {
    let language = "en";
    let title = if language.starts_with("zh") {
        "OneSpace 文件共享"
    } else {
        "OneSpace File Sharing"
    };
    let files = session
        .files
        .iter()
        .map(|file| {
            format!(
                "<li><a href=\"/s/{}/files/{}\">{}</a><span>{}</span></li>",
                session.token,
                file.id,
                html_escape(&file.name),
                format_size(file.size)
            )
        })
        .collect::<String>();
    let document = format!("<!doctype html><html lang=\"{language}\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{title}</title><style>body{{font:16px system-ui,sans-serif;margin:0;background:#f7f8fa;color:#172033}}main{{max-width:680px;margin:32px auto;padding:24px}}ul{{list-style:none;padding:0}}li{{display:flex;justify-content:space-between;gap:16px;padding:14px 0;border-bottom:1px solid #dfe3e8}}a{{color:#155eef;overflow-wrap:anywhere}}span{{color:#667085;white-space:nowrap}}</style></head><body><main><h1>{title}</h1><p>{} {}</p><ul>{files}</ul></main></body></html>", session.files.len(), if session.files.len() == 1 { "file" } else { "files" });
    security_headers(Response::builder())
        .status(StatusCode::OK)
        .header("Content-Type", "text/html; charset=utf-8")
        .header(
            "Content-Security-Policy",
            "default-src 'none'; style-src 'unsafe-inline'",
        )
        .header("Content-Length", document.len())
        .body(
            Full::new(Bytes::from(document))
                .map_err(|never: Infallible| match never {})
                .boxed(),
        )
        .expect("valid HTML response")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParsedRange {
    Bytes(u64, u64),
    Full,
    Invalid,
}

pub(crate) fn parse_range(header: Option<&str>, size: u64) -> ParsedRange {
    let Some(value) = header else {
        return ParsedRange::Full;
    };
    if value.contains(',') {
        return ParsedRange::Full;
    }
    let Some(spec) = value.strip_prefix("bytes=") else {
        return ParsedRange::Invalid;
    };
    let Some((start, end)) = spec.split_once('-') else {
        return ParsedRange::Invalid;
    };
    if size == 0 {
        return ParsedRange::Invalid;
    }
    match (start.trim(), end.trim()) {
        ("", "") => ParsedRange::Invalid,
        ("", suffix) => suffix
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .map(|suffix| {
                let length = suffix.min(size);
                ParsedRange::Bytes(size - length, size - 1)
            })
            .unwrap_or(ParsedRange::Invalid),
        (start, "") => start
            .parse::<u64>()
            .ok()
            .filter(|value| *value < size)
            .map(|start| ParsedRange::Bytes(start, size - 1))
            .unwrap_or(ParsedRange::Invalid),
        (start, end) => match (start.parse::<u64>(), end.parse::<u64>()) {
            (Ok(start), Ok(end)) if start <= end && start < size => {
                ParsedRange::Bytes(start, end.min(size - 1))
            }
            _ => ParsedRange::Invalid,
        },
    }
}

async fn file_response(
    session: SharedSession,
    file: SharedFile,
    method: &Method,
    range: Option<&str>,
    client_address: String,
) -> Response<ResponseBody> {
    let metadata = match tokio::fs::metadata(&file.path).await {
        Ok(metadata) if metadata.is_file() => metadata,
        _ => return empty(StatusCode::NOT_FOUND),
    };
    let size = metadata.len();
    let parsed = parse_range(range, size);
    if parsed == ParsedRange::Invalid {
        return security_headers(Response::builder())
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header("Content-Range", format!("bytes */{size}"))
            .body(
                Full::new(Bytes::new())
                    .map_err(|never: Infallible| match never {})
                    .boxed(),
            )
            .expect("valid 416 response");
    }
    let (start, end, status) = match parsed {
        ParsedRange::Bytes(start, end) => (start, end, StatusCode::PARTIAL_CONTENT),
        ParsedRange::Full => (0, size.saturating_sub(1), StatusCode::OK),
        ParsedRange::Invalid => unreachable!(),
    };
    let response_bytes = if size == 0 { 0 } else { end - start + 1 };
    let disposition = format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        safe_header_filename(&file.name),
        utf8_filename(&file.name)
    );
    let mut builder = security_headers(Response::builder())
        .status(status)
        .header("Content-Type", "application/octet-stream")
        .header("Content-Disposition", disposition)
        .header("Content-Length", response_bytes)
        .header("Accept-Ranges", "bytes");
    if status == StatusCode::PARTIAL_CONTENT {
        builder = builder.header("Content-Range", format!("bytes {start}-{end}/{size}"));
    }
    if method == Method::HEAD {
        return builder
            .body(
                Full::new(Bytes::new())
                    .map_err(|never: Infallible| match never {})
                    .boxed(),
            )
            .expect("valid HEAD response");
    }
    let transfer_id = begin_transfer(&session, &file, client_address, response_bytes);
    let mut handle = match tokio::fs::File::open(&file.path).await {
        Ok(handle) => handle,
        Err(error) => {
            finish_transfer(
                &session,
                &transfer_id,
                FileSharingTransferState::Failed,
                0,
                Some(error.to_string()),
            );
            return empty(StatusCode::NOT_FOUND);
        }
    };
    if handle.seek(SeekFrom::Start(start)).await.is_err() {
        finish_transfer(
            &session,
            &transfer_id,
            FileSharingTransferState::Failed,
            0,
            Some("failed to seek shared file".to_string()),
        );
        return empty(StatusCode::NOT_FOUND);
    }
    builder
        .body(BodyExt::boxed(StreamBody::new(DownloadStream::new(
            handle,
            response_bytes,
            session,
            transfer_id,
        ))))
        .expect("valid file response")
}

pub(crate) async fn handle(
    request: Request<Incoming>,
    session: SharedSession,
    client_address: String,
) -> Result<Response<ResponseBody>, Infallible> {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return Ok(security_headers(Response::builder())
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .header("Allow", "GET, HEAD")
            .body(
                Full::new(Bytes::new())
                    .map_err(|never: Infallible| match never {})
                    .boxed(),
            )
            .expect("valid 405 response"));
    }
    let pieces = request
        .uri()
        .path()
        .split('/')
        .filter(|piece| !piece.is_empty())
        .collect::<Vec<_>>();
    if pieces.len() < 2 || pieces[0] != "s" || pieces[1] != session.token {
        return Ok(empty(StatusCode::NOT_FOUND));
    }
    if pieces.len() == 2 && request.uri().path().ends_with('/') {
        return Ok(listing(&session));
    }
    if pieces.len() == 4 && pieces[2] == "files" {
        if let Some(file) = session
            .files
            .iter()
            .find(|file| file.id == pieces[3])
            .cloned()
        {
            return Ok(file_response(
                session,
                file,
                request.method(),
                request
                    .headers()
                    .get("range")
                    .and_then(|value| value.to_str().ok()),
                client_address,
            )
            .await);
        }
    }
    Ok(empty(StatusCode::NOT_FOUND))
}
