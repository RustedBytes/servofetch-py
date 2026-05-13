use std::sync::{Arc, Mutex};
use std::time::Duration;

use pyo3::exceptions::{PyRuntimeError, PyTimeoutError, PyValueError};
use pyo3::prelude::*;
use url::Url;

use crate::errors::validate_timeout;
use crate::page::Page;

pub(crate) const DEFAULT_BOOTSTRAP: &str = "128.31.0.39:9131";

#[derive(Clone)]
pub(crate) struct OnionConfig {
    bootstrap: String,
    consensus_file: String,
    verbose: bool,
    response_limit: usize,
    default_timeout_ms: i32,
    session: Arc<Mutex<Option<Arc<onionlink_core::Session>>>>,
}

impl OnionConfig {
    pub(crate) fn new(
        default_timeout: f64,
        bootstrap: String,
        consensus_file: Option<String>,
        verbose: bool,
        response_limit: usize,
    ) -> PyResult<Self> {
        if response_limit == 0 {
            return Err(PyValueError::new_err(
                "onion_response_limit must be greater than zero",
            ));
        }

        Ok(Self {
            bootstrap,
            consensus_file: consensus_file.unwrap_or_default(),
            verbose,
            response_limit,
            default_timeout_ms: timeout_to_millis(default_timeout)?,
            session: Arc::new(Mutex::new(None)),
        })
    }

    pub(crate) fn bootstrap(&self) -> String {
        self.bootstrap.clone()
    }

    pub(crate) fn consensus_file(&self) -> Option<String> {
        if self.consensus_file.is_empty() {
            None
        } else {
            Some(self.consensus_file.clone())
        }
    }

    pub(crate) fn verbose(&self) -> bool {
        self.verbose
    }

    pub(crate) fn response_limit(&self) -> usize {
        self.response_limit
    }

    fn session(&self, timeout_ms: i32) -> PyResult<Arc<onionlink_core::Session>> {
        if timeout_ms != self.default_timeout_ms {
            return self.create_session(timeout_ms);
        }

        let mut session = self
            .session
            .lock()
            .map_err(|_| PyRuntimeError::new_err("onionlink session lock was poisoned"))?;
        if let Some(existing) = session.as_ref() {
            return Ok(Arc::clone(existing));
        }

        let initialized = self.create_session(timeout_ms)?;
        *session = Some(Arc::clone(&initialized));
        Ok(initialized)
    }

    fn create_session(&self, timeout_ms: i32) -> PyResult<Arc<onionlink_core::Session>> {
        onionlink_core::Session::new(
            &self.bootstrap,
            &self.consensus_file,
            timeout_ms,
            self.verbose,
        )
        .map(Arc::new)
        .map_err(map_onion_error)
    }
}

pub(crate) struct OnionUrl {
    url: String,
    onion: String,
    port: u16,
    path: String,
    host_header: String,
}

pub(crate) fn parse_onion_url(input: &str) -> PyResult<Option<OnionUrl>> {
    let candidate = if might_be_bare_onion(input) {
        format!("http://{input}")
    } else {
        input.to_string()
    };

    let parsed = match Url::parse(&candidate) {
        Ok(parsed) => parsed,
        Err(error) if might_be_onion_input(input) => {
            return Err(PyValueError::new_err(format!(
                "invalid .onion URL {input:?}: {error}"
            )));
        }
        Err(_) => return Ok(None),
    };

    let Some(host) = parsed.host_str() else {
        return Ok(None);
    };
    if !host.to_ascii_lowercase().ends_with(".onion") {
        return Ok(None);
    }
    if parsed.scheme() != "http" {
        return Err(PyValueError::new_err(format!(
            "onionlink backend supports http:// .onion URLs only, got {:?}",
            parsed.scheme()
        )));
    }

    let onion = host.to_ascii_lowercase();
    let port = parsed.port_or_known_default().unwrap_or(80);
    let mut path = parsed.path().to_string();
    if path.is_empty() {
        path.push('/');
    }
    if let Some(query) = parsed.query() {
        path.push('?');
        path.push_str(query);
    }
    let host_header = if parsed.port().is_some() {
        format!("{onion}:{port}")
    } else {
        onion.clone()
    };

    Ok(Some(OnionUrl {
        url: parsed.to_string(),
        onion,
        port,
        path,
        host_header,
    }))
}

pub(crate) fn fetch_onion_page(
    config: &OnionConfig,
    onion_url: OnionUrl,
    timeout: f64,
    user_agent: Option<&str>,
) -> PyResult<Page> {
    let timeout_ms = timeout_to_millis(timeout)?;
    let session = config.session(timeout_ms)?;
    let request = build_http_get(&onion_url, user_agent);
    let raw = session
        .request(
            &onion_url.onion,
            onion_url.port,
            request.as_bytes(),
            config.response_limit,
        )
        .map_err(map_onion_error)?;
    let body = onionlink_core::decode_http_body(&raw).map_err(map_onion_error)?;
    let html = String::from_utf8_lossy(&body).into_owned();
    Ok(Page::from_html(onion_url.url, html))
}

pub(crate) fn timeout_to_millis(timeout: f64) -> PyResult<i32> {
    validate_timeout(timeout)?;
    let millis = Duration::from_secs_f64(timeout).as_millis().max(1);
    if millis > i32::MAX as u128 {
        return Err(PyValueError::new_err(
            "onion timeout must be less than or equal to i32::MAX milliseconds",
        ));
    }
    Ok(millis as i32)
}

fn build_http_get(onion_url: &OnionUrl, user_agent: Option<&str>) -> String {
    let user_agent = user_agent
        .filter(|value| !value.trim().is_empty())
        .map(sanitize_header_value)
        .unwrap_or_else(|| format!("servofetch/{}", env!("CARGO_PKG_VERSION")));

    format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\nUser-Agent: {}\r\nAccept: text/html,application/xhtml+xml,text/plain,*/*;q=0.8\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n",
        onion_url.path, onion_url.host_header, user_agent
    )
}

fn sanitize_header_value(value: &str) -> String {
    value
        .chars()
        .filter_map(|ch| match ch {
            '\r' | '\n' => Some(' '),
            ch if ch.is_control() => None,
            ch => Some(ch),
        })
        .collect()
}

fn might_be_bare_onion(input: &str) -> bool {
    !input.contains("://") && host_candidate(input).ends_with(".onion")
}

fn might_be_onion_input(input: &str) -> bool {
    input.to_ascii_lowercase().contains(".onion")
}

fn host_candidate(input: &str) -> String {
    let authority = input
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(input)
        .rsplit('@')
        .next()
        .unwrap_or(input);
    authority
        .split(':')
        .next()
        .unwrap_or(authority)
        .to_ascii_lowercase()
}

fn map_onion_error(error: onionlink_core::Error) -> PyErr {
    let message = format!("onionlink request failed: {error}");
    if message.to_ascii_lowercase().contains("timed out") {
        PyTimeoutError::new_err(message)
    } else {
        PyRuntimeError::new_err(message)
    }
}

#[cfg(test)]
mod tests {
    use super::{build_http_get, parse_onion_url};

    const ONION: &str = "archiveiya74codqgiixo33q62qlrqtkgmcitqx5u2oeqnmn5bpcbiyd.onion";

    #[test]
    fn parses_http_onion_url() {
        let parsed = parse_onion_url(&format!("http://{ONION}/search?q=test"))
            .unwrap()
            .unwrap();

        assert_eq!(parsed.onion, ONION);
        assert_eq!(parsed.port, 80);
        assert_eq!(parsed.path, "/search?q=test");
        assert_eq!(parsed.host_header, ONION);
    }

    #[test]
    fn parses_bare_onion_url() {
        let parsed = parse_onion_url(&format!("{ONION}:8080/docs"))
            .unwrap()
            .unwrap();

        assert_eq!(parsed.url, format!("http://{ONION}:8080/docs"));
        assert_eq!(parsed.port, 8080);
        assert_eq!(parsed.path, "/docs");
        assert_eq!(parsed.host_header, format!("{ONION}:8080"));
    }

    #[test]
    fn ignores_non_onion_urls() {
        assert!(parse_onion_url("https://example.com").unwrap().is_none());
    }

    #[test]
    fn builds_identity_http_get_request() {
        let parsed = parse_onion_url(&format!("http://{ONION}/"))
            .unwrap()
            .unwrap();
        let request = build_http_get(&parsed, Some("test-agent\r\nX-Bad: yes"));

        assert!(request.starts_with("GET / HTTP/1.0\r\n"));
        assert!(request.contains(&format!("Host: {ONION}\r\n")));
        assert!(request.contains("Accept-Encoding: identity\r\n"));
        assert!(!request.contains("\r\nX-Bad: yes\r\n"));
    }
}
