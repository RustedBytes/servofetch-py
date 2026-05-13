/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::sync::{Arc, Mutex, OnceLock};

use http::header::{ACCEPT_ENCODING, CONNECTION, CONTENT_LENGTH, HOST, TRANSFER_ENCODING};
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper::Response as HyperResponse;
use hyper::body::Bytes as HyperBytes;
use hyper::ext::ReasonPhrase;
use net_traits::NetworkError;
use servo_url::ServoUrl;
use tokio::sync::mpsc::UnboundedSender;

use crate::connector::BoxedBody;

const DEFAULT_BOOTSTRAP: &str = "128.31.0.39:9131";
const DEFAULT_TIMEOUT_MS: i32 = 30_000;
const DEFAULT_RESPONSE_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OnionTransportConfig {
    pub bootstrap: String,
    pub consensus_file: String,
    pub timeout_ms: i32,
    pub verbose: bool,
    pub response_limit: usize,
}

impl Default for OnionTransportConfig {
    fn default() -> Self {
        Self {
            bootstrap: DEFAULT_BOOTSTRAP.to_string(),
            consensus_file: String::new(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            verbose: false,
            response_limit: DEFAULT_RESPONSE_LIMIT,
        }
    }
}

struct OnionTransportState {
    config: OnionTransportConfig,
    session: Option<Arc<onionlink_core::Session>>,
}

impl Default for OnionTransportState {
    fn default() -> Self {
        Self {
            config: OnionTransportConfig::default(),
            session: None,
        }
    }
}

fn state() -> &'static Mutex<OnionTransportState> {
    static STATE: OnceLock<Mutex<OnionTransportState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(OnionTransportState::default()))
}

pub fn configure_transport(config: OnionTransportConfig) -> Result<(), String> {
    if config.timeout_ms <= 0 {
        return Err("onion timeout must be a positive number of milliseconds".to_string());
    }
    if config.response_limit == 0 {
        return Err("onion response limit must be greater than zero".to_string());
    }

    let mut state = state()
        .lock()
        .map_err(|_| "onion transport lock was poisoned".to_string())?;
    if state.session.is_some() && state.config != config {
        return Err(
            "onion transport is process-wide and was already initialized with different options"
                .to_string(),
        );
    }
    state.config = config;
    Ok(())
}

pub fn is_onion_url(url: &ServoUrl) -> bool {
    url.host_str()
        .is_some_and(|host| host.to_ascii_lowercase().ends_with(".onion"))
}

pub async fn obtain_response(
    url: &ServoUrl,
    method: &Method,
    request_headers: &HeaderMap,
    has_streaming_body: bool,
    fetch_terminated: UnboundedSender<bool>,
) -> Result<HyperResponse<BoxedBody>, NetworkError> {
    if url.scheme() != "http" {
        return Err(NetworkError::UnsupportedScheme);
    }
    if has_streaming_body {
        let _ = fetch_terminated.send(true);
        return Err(NetworkError::ResourceLoadError(
            "onionlink-backed Servo networking does not support streaming request bodies"
                .to_string(),
        ));
    }

    let url = url.clone();
    let method = method.clone();
    let request_headers = request_headers.clone();
    tokio::task::spawn_blocking(move || blocking_obtain_response(&url, &method, &request_headers))
        .await
        .map_err(|err| NetworkError::ResourceLoadError(format!("onion task failed: {err}")))?
}

fn blocking_obtain_response(
    url: &ServoUrl,
    method: &Method,
    request_headers: &HeaderMap,
) -> Result<HyperResponse<BoxedBody>, NetworkError> {
    let config = {
        let state = state()
            .lock()
            .map_err(|_| NetworkError::ResourceLoadError("onion transport lock was poisoned".into()))?;
        state.config.clone()
    };
    let session = session_for_config(&config)?;
    let onion = url
        .host_str()
        .ok_or(NetworkError::UnsupportedScheme)?
        .to_ascii_lowercase();
    let port = url.port_or_known_default().unwrap_or(80);
    let payload = build_http_request(url, method, request_headers)?;
    let raw = session
        .request(&onion, port, &payload, config.response_limit)
        .map_err(|err| NetworkError::ResourceLoadError(format!("onionlink request failed: {err}")))?;

    parse_http_response(raw)
}

fn session_for_config(
    config: &OnionTransportConfig,
) -> Result<Arc<onionlink_core::Session>, NetworkError> {
    let mut state = state()
        .lock()
        .map_err(|_| NetworkError::ResourceLoadError("onion transport lock was poisoned".into()))?;
    if let Some(session) = state.session.as_ref() {
        return Ok(Arc::clone(session));
    }

    let session = onionlink_core::Session::new(
        &config.bootstrap,
        &config.consensus_file,
        config.timeout_ms,
        config.verbose,
    )
    .map(Arc::new)
    .map_err(|err| NetworkError::ResourceLoadError(format!("onionlink bootstrap failed: {err}")))?;
    state.session = Some(Arc::clone(&session));
    Ok(session)
}

fn build_http_request(
    url: &ServoUrl,
    method: &Method,
    request_headers: &HeaderMap,
) -> Result<Vec<u8>, NetworkError> {
    let mut path = url.path().to_string();
    if path.is_empty() {
        path.push('/');
    }
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }

    let host = url.host_str().ok_or(NetworkError::UnsupportedScheme)?;
    let host_header = if let Some(port) = url.port() {
        format!("{host}:{port}")
    } else {
        host.to_ascii_lowercase()
    };

    let mut payload = Vec::new();
    payload.extend_from_slice(method.as_str().as_bytes());
    payload.extend_from_slice(b" ");
    payload.extend_from_slice(path.as_bytes());
    payload.extend_from_slice(b" HTTP/1.0\r\nHost: ");
    payload.extend_from_slice(host_header.as_bytes());
    payload.extend_from_slice(b"\r\nConnection: close\r\nAccept-Encoding: identity\r\n");

    for (name, value) in request_headers {
        if matches!(
            *name,
            HOST | CONNECTION | TRANSFER_ENCODING | ACCEPT_ENCODING | CONTENT_LENGTH
        ) {
            continue;
        }
        payload.extend_from_slice(name.as_str().as_bytes());
        payload.extend_from_slice(b": ");
        payload.extend_from_slice(sanitize_header_value(value.as_bytes()).as_slice());
        payload.extend_from_slice(b"\r\n");
    }

    payload.extend_from_slice(b"\r\n");
    Ok(payload)
}

fn sanitize_header_value(value: &[u8]) -> Vec<u8> {
    value
        .iter()
        .copied()
        .filter_map(|byte| match byte {
            b'\r' | b'\n' => Some(b' '),
            0..=31 | 127 => None,
            byte => Some(byte),
        })
        .collect()
}

fn parse_http_response(raw: Vec<u8>) -> Result<HyperResponse<BoxedBody>, NetworkError> {
    let (head, body) = split_head_body(&raw).ok_or_else(|| {
        NetworkError::ResourceLoadError("onion service returned a malformed HTTP response".into())
    })?;
    let mut lines = head.split(|byte| *byte == b'\n');
    let status_line = trim_line(lines.next().unwrap_or_default());
    let (status, reason) = parse_status_line(status_line)?;
    let mut headers = HeaderMap::new();

    for line in lines {
        let line = trim_line(line);
        if line.is_empty() {
            continue;
        }
        let Some(colon) = line.iter().position(|byte| *byte == b':') else {
            continue;
        };
        let name = HeaderName::from_bytes(&line[..colon]).map_err(|err| {
            NetworkError::ResourceLoadError(format!("onion response has invalid header name: {err}"))
        })?;
        let value = HeaderValue::from_bytes(trim_ascii(&line[colon + 1..])).map_err(|err| {
            NetworkError::ResourceLoadError(format!("onion response has invalid header value: {err}"))
        })?;
        headers.append(name, value);
    }

    let mut body = body.to_vec();
    if has_chunked_transfer_encoding(&headers) {
        body = decode_chunked_body(&body)?;
        headers.remove(TRANSFER_ENCODING);
        headers.remove(CONTENT_LENGTH);
    }
    if !headers.contains_key(CONTENT_LENGTH) {
        let value = HeaderValue::from_str(&body.len().to_string()).map_err(|err| {
            NetworkError::ResourceLoadError(format!("failed to build content-length header: {err}"))
        })?;
        headers.insert(CONTENT_LENGTH, value);
    }

    let boxed_body = Full::new(HyperBytes::from(body))
        .map_err(|_| unreachable!())
        .boxed();
    let mut response = HyperResponse::builder()
        .status(status)
        .body(boxed_body)
        .map_err(|err| NetworkError::ResourceLoadError(format!("failed to build response: {err}")))?;
    *response.headers_mut() = headers;
    if !reason.is_empty() {
        if let Ok(reason) = ReasonPhrase::try_from(reason.to_vec()) {
            response.extensions_mut().insert(reason);
        }
    }
    Ok(response)
}

fn split_head_body(raw: &[u8]) -> Option<(&[u8], &[u8])> {
    if let Some(pos) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
        return Some((&raw[..pos], &raw[pos + 4..]));
    }
    raw.windows(2)
        .position(|window| window == b"\n\n")
        .map(|pos| (&raw[..pos], &raw[pos + 2..]))
}

fn parse_status_line(line: &[u8]) -> Result<(StatusCode, &[u8]), NetworkError> {
    let mut parts = line.splitn(3, |byte| *byte == b' ');
    let version = parts.next().unwrap_or_default();
    if !version.starts_with(b"HTTP/") {
        return Err(NetworkError::ResourceLoadError(
            "onion service returned a response without an HTTP status line".into(),
        ));
    }
    let status = parts.next().unwrap_or_default();
    let status = std::str::from_utf8(status)
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .and_then(|value| StatusCode::from_u16(value).ok())
        .ok_or_else(|| {
            NetworkError::ResourceLoadError("onion service returned an invalid HTTP status".into())
        })?;
    let reason = parts.next().unwrap_or_default();
    Ok((status, reason))
}

fn has_chunked_transfer_encoding(headers: &HeaderMap) -> bool {
    headers.get_all(TRANSFER_ENCODING).iter().any(|value| {
        value
            .to_str()
            .is_ok_and(|value| value.to_ascii_lowercase().contains("chunked"))
    })
}

fn decode_chunked_body(body: &[u8]) -> Result<Vec<u8>, NetworkError> {
    let mut decoded = Vec::new();
    let mut pos = 0;

    loop {
        let (line_end, sep_len) = find_line_end(body, pos).ok_or_else(|| {
            NetworkError::ResourceLoadError("onion response has a truncated chunk header".into())
        })?;
        let size_text = trim_ascii(&body[pos..line_end])
            .split(|byte| *byte == b';')
            .next()
            .unwrap_or_default();
        let size = std::str::from_utf8(size_text)
            .ok()
            .and_then(|value| usize::from_str_radix(value.trim(), 16).ok())
            .ok_or_else(|| {
                NetworkError::ResourceLoadError("onion response has an invalid chunk size".into())
            })?;
        pos = line_end + sep_len;
        if size == 0 {
            return Ok(decoded);
        }
        if pos + size > body.len() {
            return Err(NetworkError::ResourceLoadError(
                "onion response has a truncated chunk body".into(),
            ));
        }
        decoded.extend_from_slice(&body[pos..pos + size]);
        pos += size;
        if body.get(pos..pos + 2) == Some(b"\r\n") {
            pos += 2;
        } else if body.get(pos..pos + 1) == Some(b"\n") {
            pos += 1;
        }
    }
}

fn find_line_end(body: &[u8], start: usize) -> Option<(usize, usize)> {
    let mut pos = start;
    while pos < body.len() {
        if body.get(pos..pos + 2) == Some(b"\r\n") {
            return Some((pos, 2));
        }
        if body[pos] == b'\n' {
            return Some((pos, 1));
        }
        pos += 1;
    }
    None
}

fn trim_line(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

fn trim_ascii(mut value: &[u8]) -> &[u8] {
    while value.first().is_some_and(u8::is_ascii_whitespace) {
        value = &value[1..];
    }
    while value.last().is_some_and(u8::is_ascii_whitespace) {
        value = &value[..value.len() - 1];
    }
    value
}
