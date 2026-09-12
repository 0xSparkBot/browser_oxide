//! HTTP/1.1 fallback client using httparse.
//!
//! Used when ALPN negotiates `http/1.1` instead of `h2`.

use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::net::error::NetError;

/// Raw HTTP/1.1 response before decompression.
pub struct RawResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub(crate) timing: crate::net::WireTiming,
}

/// HTTP/1.1 response headers plus a decoded transfer-body chunk stream.
pub struct RawStreamingResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: mpsc::UnboundedReceiver<Result<Vec<u8>, String>>,
}

/// Send an HTTP/1.1 GET request over a stream.
pub async fn send_get<S>(
    stream: &mut S,
    authority: &str,
    path: &str,
    headers: &[(String, String)],
) -> Result<RawResponse, NetError>
where
    S: AsyncReadExt + AsyncWriteExt + Unpin,
{
    send_request(stream, "GET", authority, path, headers, None).await
}

/// Send an HTTP/1.1 GET and return as soon as response headers are available.
/// Transfer-encoding is decoded incrementally on a background task.
pub async fn send_get_stream<S>(
    mut stream: S,
    authority: &str,
    path: &str,
    headers: &[(String, String)],
) -> Result<RawStreamingResponse, NetError>
where
    S: AsyncReadExt + AsyncWriteExt + Unpin + Send + 'static,
{
    let mut request = format!("GET {path} HTTP/1.1\r\n");
    request.push_str(&format!("Host: {authority}\r\n"));
    request.push_str("Connection: keep-alive\r\n");
    for (name, value) in headers {
        let lower = name.to_lowercase();
        if lower == "host" || lower == "connection" || lower.starts_with(':') {
            continue;
        }
        request.push_str(&format!(
            "{}: {}\r\n",
            normalize_h1_header_name(name),
            value
        ));
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| NetError::Http(format!("failed to write request: {e}")))?;
    stream
        .flush()
        .await
        .map_err(|e| NetError::Http(format!("failed to flush request: {e}")))?;

    let mut buf = Vec::with_capacity(8192);
    let header_len = loop {
        let mut tmp = [0u8; 4096];
        let n = stream
            .read(&mut tmp)
            .await
            .map_err(|e| NetError::Http(format!("read error: {e}")))?;
        if n == 0 {
            return Err(NetError::Http(
                "connection closed before headers".to_string(),
            ));
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_header_end(&buf) {
            break pos + 4;
        }
        if buf.len() > 65536 {
            return Err(NetError::Http("headers too large".to_string()));
        }
    };

    let mut parsed_headers = [httparse::EMPTY_HEADER; 128];
    let mut response = httparse::Response::new(&mut parsed_headers);
    response
        .parse(&buf[..header_len])
        .map_err(|e| NetError::Http(format!("failed to parse response: {e}")))?;
    let status = response.code.unwrap_or(0);
    let status_text = response.reason.unwrap_or("").to_string();
    let mut response_headers = Vec::new();
    let mut content_length = None;
    let mut chunked = false;
    for header in response.headers.iter() {
        let name = header.name.to_lowercase();
        let value = String::from_utf8_lossy(header.value).to_string();
        if name == "content-length" {
            content_length = value.parse::<usize>().ok();
        }
        if name == "transfer-encoding" && value.to_ascii_lowercase().contains("chunked") {
            chunked = true;
        }
        response_headers.push((name, value));
    }
    let initial = buf[header_len..].to_vec();
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let result = if chunked {
            stream_chunked_body(stream, initial, &tx).await
        } else if let Some(len) = content_length {
            stream_content_length_body(stream, initial, len, &tx).await
        } else {
            stream_until_close(stream, initial, &tx).await
        };
        if let Err(error) = result {
            let _ = tx.send(Err(error));
        }
    });

    Ok(RawStreamingResponse {
        status,
        status_text,
        headers: response_headers,
        body: rx,
    })
}

/// Send an HTTP/1.1 POST request over a stream.
pub async fn send_post<S>(
    stream: &mut S,
    authority: &str,
    path: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> Result<RawResponse, NetError>
where
    S: AsyncReadExt + AsyncWriteExt + Unpin,
{
    send_request(stream, "POST", authority, path, headers, Some(body)).await
}

async fn send_request<S>(
    stream: &mut S,
    method: &str,
    authority: &str,
    path: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
) -> Result<RawResponse, NetError>
where
    S: AsyncReadExt + AsyncWriteExt + Unpin,
{
    // Build the request using Title-Case names and specific order for H1.
    // Real Chrome H1 order: Host, Connection, (Content-Length), then others.
    let mut request = format!("{method} {path} HTTP/1.1\r\n");
    request.push_str(&format!("Host: {authority}\r\n"));
    request.push_str("Connection: keep-alive\r\n");

    if let Some(body) = body {
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }

    for (name, value) in headers {
        let lk = name.to_lowercase();
        // Skip headers we already added or pseudo-headers from H2 layer
        if lk == "host" || lk == "connection" || lk == "content-length" || lk.starts_with(':') {
            continue;
        }

        // Normalize casing to Title-Case for H1 (e.g., user-agent -> User-Agent)
        let h1_name = normalize_h1_header_name(name);
        request.push_str(&format!("{h1_name}: {value}\r\n"));
    }
    request.push_str("\r\n");

    let request_start = Instant::now();
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| NetError::Http(format!("failed to write request: {e}")))?;

    if let Some(body) = body {
        stream
            .write_all(body)
            .await
            .map_err(|e| NetError::Http(format!("failed to write body: {e}")))?;
    }

    stream
        .flush()
        .await
        .map_err(|e| NetError::Http(format!("failed to flush: {e}")))?;

    // Read the response
    read_response(stream, request_start).await
}

async fn read_response<S>(stream: &mut S, request_start: Instant) -> Result<RawResponse, NetError>
where
    S: AsyncReadExt + Unpin,
{
    let mut buf = Vec::with_capacity(8192);
    let header_len;
    let response_start;

    // Read until we find the end of headers (\r\n\r\n)
    loop {
        let mut tmp = [0u8; 4096];
        let n = stream
            .read(&mut tmp)
            .await
            .map_err(|e| NetError::Http(format!("read error: {e}")))?;
        if n == 0 {
            return Err(NetError::Http(
                "connection closed before headers".to_string(),
            ));
        }
        buf.extend_from_slice(&tmp[..n]);

        if let Some(pos) = find_header_end(&buf) {
            header_len = pos + 4; // include \r\n\r\n
            response_start = Instant::now();
            break;
        }

        if buf.len() > 65536 {
            return Err(NetError::Http("headers too large".to_string()));
        }
    }

    // Parse headers
    let mut parsed_headers = [httparse::EMPTY_HEADER; 128];
    let mut response = httparse::Response::new(&mut parsed_headers);
    response
        .parse(&buf[..header_len])
        .map_err(|e| NetError::Http(format!("failed to parse response: {e}")))?;

    let status = response.code.unwrap_or(0);
    let status_text = response.reason.unwrap_or("").to_string();

    let mut headers: Vec<(String, String)> = Vec::new();
    let mut content_length: Option<usize> = None;
    let mut chunked = false;

    for header in response.headers.iter() {
        let name = header.name.to_lowercase();
        let value = String::from_utf8_lossy(header.value).to_string();
        if name == "content-length" {
            content_length = value.parse().ok();
        }
        if name == "transfer-encoding" && value.contains("chunked") {
            chunked = true;
        }
        headers.push((name, value));
    }

    // Read the body
    let body_start = &buf[header_len..];
    let body = if chunked {
        read_chunked_body(stream, body_start).await?
    } else if let Some(len) = content_length {
        read_content_length_body(stream, body_start, len).await?
    } else {
        // Read until connection close
        read_until_close(stream, body_start).await?
    };

    let response_end = Instant::now();
    Ok(RawResponse {
        status,
        status_text,
        headers,
        body,
        timing: crate::net::WireTiming {
            request_start,
            response_start,
            response_end,
        },
    })
}

async fn stream_content_length_body<S>(
    mut stream: S,
    initial: Vec<u8>,
    len: usize,
    tx: &mpsc::UnboundedSender<Result<Vec<u8>, String>>,
) -> Result<(), String>
where
    S: AsyncReadExt + Unpin,
{
    let mut remaining = len;
    if !initial.is_empty() {
        let take = initial.len().min(remaining);
        if take > 0 && tx.send(Ok(initial[..take].to_vec())).is_err() {
            return Ok(());
        }
        remaining -= take;
    }
    let mut buf = [0u8; 8192];
    while remaining > 0 {
        let cap = remaining.min(buf.len());
        let n = stream
            .read(&mut buf[..cap])
            .await
            .map_err(|e| format!("body read error: {e}"))?;
        if n == 0 {
            return Err("unexpected EOF in content-length body".to_string());
        }
        if tx.send(Ok(buf[..n].to_vec())).is_err() {
            return Ok(());
        }
        remaining -= n;
    }
    Ok(())
}

async fn stream_until_close<S>(
    mut stream: S,
    initial: Vec<u8>,
    tx: &mpsc::UnboundedSender<Result<Vec<u8>, String>>,
) -> Result<(), String>
where
    S: AsyncReadExt + Unpin,
{
    if !initial.is_empty() && tx.send(Ok(initial)).is_err() {
        return Ok(());
    }
    let mut buf = [0u8; 8192];
    loop {
        let n = stream
            .read(&mut buf)
            .await
            .map_err(|e| format!("body read error: {e}"))?;
        if n == 0 {
            return Ok(());
        }
        if tx.send(Ok(buf[..n].to_vec())).is_err() {
            return Ok(());
        }
    }
}

async fn stream_chunked_body<S>(
    mut stream: S,
    mut raw: Vec<u8>,
    tx: &mpsc::UnboundedSender<Result<Vec<u8>, String>>,
) -> Result<(), String>
where
    S: AsyncReadExt + Unpin,
{
    loop {
        let line_end = loop {
            if let Some(pos) = raw.windows(2).position(|w| w == b"\r\n") {
                break pos;
            }
            let mut buf = [0u8; 4096];
            let n = stream
                .read(&mut buf)
                .await
                .map_err(|e| format!("chunked read error: {e}"))?;
            if n == 0 {
                return Err("unexpected EOF in chunked body".to_string());
            }
            raw.extend_from_slice(&buf[..n]);
        };
        let line = String::from_utf8_lossy(&raw[..line_end]);
        let size_text = line.split(';').next().unwrap_or("").trim();
        let chunk_size = usize::from_str_radix(size_text, 16)
            .map_err(|e| format!("invalid chunk size '{size_text}': {e}"))?;
        raw.drain(..line_end + 2);
        if chunk_size == 0 {
            return Ok(());
        }
        while raw.len() < chunk_size + 2 {
            let mut buf = [0u8; 8192];
            let n = stream
                .read(&mut buf)
                .await
                .map_err(|e| format!("chunk data read error: {e}"))?;
            if n == 0 {
                return Err("unexpected EOF in chunked body".to_string());
            }
            raw.extend_from_slice(&buf[..n]);
        }
        if &raw[chunk_size..chunk_size + 2] != b"\r\n" {
            return Err("malformed chunked encoding: missing chunk CRLF".to_string());
        }
        let chunk = raw[..chunk_size].to_vec();
        raw.drain(..chunk_size + 2);
        if !chunk.is_empty() && tx.send(Ok(chunk)).is_err() {
            return Ok(());
        }
    }
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

/// Standard Chrome H1 header casing normalization.
/// H2 uses lowercase exclusively, but H1 uses Title-Case.
fn normalize_h1_header_name(name: &str) -> String {
    match name.to_lowercase().as_str() {
        "upgrade-insecure-requests" => "Upgrade-Insecure-Requests".to_string(),
        "user-agent" => "User-Agent".to_string(),
        "accept" => "Accept".to_string(),
        "accept-encoding" => "Accept-Encoding".to_string(),
        "accept-language" => "Accept-Language".to_string(),
        "referer" => "Referer".to_string(),
        "cookie" => "Cookie".to_string(),
        "origin" => "Origin".to_string(),
        "priority" => "Priority".to_string(),
        "content-type" => "Content-Type".to_string(),
        s if s.starts_with("sec-") => {
            // sec-ch-ua -> Sec-Ch-Ua, sec-fetch-site -> Sec-Fetch-Site
            let parts: Vec<String> = s
                .split('-')
                .map(|p| {
                    let mut c = p.chars();
                    match c.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    }
                })
                .collect();
            parts.join("-")
        }
        _ => {
            // Default to capitalized first letter of each part
            let parts: Vec<String> = name
                .split('-')
                .map(|p| {
                    let mut c = p.chars();
                    match c.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    }
                })
                .collect();
            parts.join("-")
        }
    }
}

async fn read_content_length_body<S>(
    stream: &mut S,
    initial: &[u8],
    content_length: usize,
) -> Result<Vec<u8>, NetError>
where
    S: AsyncReadExt + Unpin,
{
    let mut body = Vec::with_capacity(content_length);
    body.extend_from_slice(initial);

    while body.len() < content_length {
        let mut tmp = vec![0u8; std::cmp::min(8192, content_length - body.len())];
        let n = stream
            .read(&mut tmp)
            .await
            .map_err(|e| NetError::Http(format!("body read error: {e}")))?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }

    Ok(body)
}

async fn read_chunked_body<S>(stream: &mut S, initial: &[u8]) -> Result<Vec<u8>, NetError>
where
    S: AsyncReadExt + Unpin,
{
    let mut raw = Vec::from(initial);
    let mut decoded = Vec::new();

    loop {
        // Ensure we have enough data for the next chunk header
        while !contains_crlf(&raw) {
            let mut tmp = [0u8; 4096];
            let n = stream
                .read(&mut tmp)
                .await
                .map_err(|e| NetError::Http(format!("chunked read error: {e}")))?;
            if n == 0 {
                return Ok(decoded);
            }
            raw.extend_from_slice(&tmp[..n]);
        }

        // Parse chunk size
        let crlf_pos = match raw.windows(2).position(|w| w == b"\r\n") {
            Some(pos) => pos,
            None => {
                return Err(NetError::Http(
                    "malformed chunked encoding: missing CRLF".into(),
                ))
            }
        };
        let size_str = std::str::from_utf8(&raw[..crlf_pos])
            .map_err(|e| NetError::Http(format!("invalid chunk size: {e}")))?;
        // Handle chunk extensions (size;ext=val)
        let size_str = size_str.split(';').next().unwrap_or(size_str).trim();
        let chunk_size = usize::from_str_radix(size_str, 16)
            .map_err(|e| NetError::Http(format!("invalid chunk size '{size_str}': {e}")))?;

        if chunk_size == 0 {
            break; // Last chunk
        }

        // Consume the size line
        raw = raw[crlf_pos + 2..].to_vec();

        // Read chunk data
        while raw.len() < chunk_size + 2 {
            let mut tmp = [0u8; 8192];
            let n = stream
                .read(&mut tmp)
                .await
                .map_err(|e| NetError::Http(format!("chunk data read error: {e}")))?;
            if n == 0 {
                return Err(NetError::Http("unexpected EOF in chunked body".to_string()));
            }
            raw.extend_from_slice(&tmp[..n]);
        }

        decoded.extend_from_slice(&raw[..chunk_size]);
        raw = raw[chunk_size + 2..].to_vec(); // skip data + trailing \r\n
    }

    Ok(decoded)
}

fn contains_crlf(buf: &[u8]) -> bool {
    buf.windows(2).any(|w| w == b"\r\n")
}

async fn read_until_close<S>(stream: &mut S, initial: &[u8]) -> Result<Vec<u8>, NetError>
where
    S: AsyncReadExt + Unpin,
{
    let mut body = Vec::from(initial);
    loop {
        let mut tmp = [0u8; 8192];
        let n = stream
            .read(&mut tmp)
            .await
            .map_err(|e| NetError::Http(format!("read error: {e}")))?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    Ok(body)
}
