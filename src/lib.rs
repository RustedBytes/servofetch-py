use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use pyo3::exceptions::{
    PyOSError, PyPermissionError, PyRuntimeError, PyTimeoutError, PyValueError,
};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

static NETWORK_POLICY: OnceLock<bool> = OnceLock::new();
static INIT_LOCK: Mutex<()> = Mutex::new(());

#[pyclass(module = "servofetch", frozen, skip_from_py_object)]
#[derive(Clone)]
struct ConsoleMessage {
    #[pyo3(get)]
    level: String,
    #[pyo3(get)]
    message: String,
}

impl From<servo_fetch::ConsoleMessage> for ConsoleMessage {
    fn from(message: servo_fetch::ConsoleMessage) -> Self {
        Self {
            level: message.level.as_str().to_string(),
            message: message.message,
        }
    }
}

#[pymethods]
impl ConsoleMessage {
    fn __repr__(&self) -> String {
        format!(
            "ConsoleMessage(level={:?}, message={:?})",
            self.level, self.message
        )
    }
}

#[pyclass(module = "servofetch", frozen, skip_from_py_object)]
#[derive(Clone)]
struct Page {
    #[pyo3(get)]
    html: String,
    #[pyo3(get)]
    inner_text: String,
    #[pyo3(get)]
    title: Option<String>,
    #[pyo3(get)]
    layout_json: Option<String>,
    #[pyo3(get)]
    js_result: Option<String>,
    #[pyo3(get)]
    accessibility_tree: Option<String>,
    #[pyo3(get)]
    console_messages: Vec<ConsoleMessage>,
    screenshot_png: Option<Vec<u8>>,
}

impl Page {
    fn from_servo(page: servo_fetch::Page) -> Self {
        let screenshot_png = page.screenshot_png().map(<[u8]>::to_vec);
        Self {
            html: page.html,
            inner_text: page.inner_text,
            title: page.title,
            layout_json: page.layout_json,
            js_result: page.js_result,
            console_messages: page
                .console_messages
                .into_iter()
                .map(ConsoleMessage::from)
                .collect(),
            accessibility_tree: page.accessibility_tree,
            screenshot_png,
        }
    }

    fn extract_input<'a>(&'a self, url: &'a str) -> servo_fetch::extract::ExtractInput<'a> {
        servo_fetch::extract::ExtractInput::new(&self.html, url)
            .with_layout_json(self.layout_json.as_deref())
            .with_inner_text(Some(&self.inner_text))
    }
}

#[pymethods]
impl Page {
    #[getter]
    fn screenshot_png<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.screenshot_png
            .as_deref()
            .map(|bytes| PyBytes::new(py, bytes))
    }

    #[pyo3(signature = (url = "", selector = None))]
    fn markdown(&self, url: &str, selector: Option<&str>) -> PyResult<String> {
        let input = self.extract_input(url).with_selector(selector);
        servo_fetch::extract::extract_text(&input).map_err(map_extract_error)
    }

    #[pyo3(signature = (url = "", selector = None))]
    fn extract_json(&self, url: &str, selector: Option<&str>) -> PyResult<String> {
        let input = self.extract_input(url).with_selector(selector);
        servo_fetch::extract::extract_json(&input).map_err(map_extract_error)
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("html", &self.html)?;
        dict.set_item("inner_text", &self.inner_text)?;
        dict.set_item("title", &self.title)?;
        dict.set_item("layout_json", &self.layout_json)?;
        dict.set_item("js_result", &self.js_result)?;
        dict.set_item("accessibility_tree", &self.accessibility_tree)?;
        dict.set_item("screenshot_png", self.screenshot_png(py))?;

        let console: Vec<_> = self
            .console_messages
            .iter()
            .map(|message| {
                let entry = PyDict::new(py);
                entry.set_item("level", &message.level)?;
                entry.set_item("message", &message.message)?;
                Ok(entry)
            })
            .collect::<PyResult<_>>()?;
        dict.set_item("console_messages", console)?;
        Ok(dict)
    }

    fn __repr__(&self) -> String {
        format!(
            "Page(title={:?}, html_len={}, inner_text_len={})",
            self.title,
            self.html.len(),
            self.inner_text.len()
        )
    }
}

#[derive(Clone)]
struct BrowserConfig {
    timeout: f64,
    settle_ms: u64,
    user_agent: Option<String>,
    allow_private_addresses: bool,
}

#[pyclass(module = "servofetch", frozen, skip_from_py_object)]
#[derive(Clone)]
struct Browser {
    config: BrowserConfig,
}

#[pymethods]
impl Browser {
    #[new]
    #[pyo3(signature = (timeout = 30.0, settle_ms = 0, user_agent = None, allow_private_addresses = false))]
    fn new(
        timeout: f64,
        settle_ms: u64,
        user_agent: Option<String>,
        allow_private_addresses: bool,
    ) -> PyResult<Self> {
        ensure_network_policy(allow_private_addresses)?;
        validate_timeout(timeout)?;
        Ok(Self {
            config: BrowserConfig {
                timeout,
                settle_ms,
                user_agent,
                allow_private_addresses,
            },
        })
    }

    #[getter]
    fn timeout(&self) -> f64 {
        self.config.timeout
    }

    #[getter]
    fn settle_ms(&self) -> u64 {
        self.config.settle_ms
    }

    #[getter]
    fn user_agent(&self) -> Option<String> {
        self.config.user_agent.clone()
    }

    #[getter]
    fn allow_private_addresses(&self) -> bool {
        self.config.allow_private_addresses
    }

    #[pyo3(signature = (url, *, timeout = None, settle_ms = None, user_agent = None, javascript = None))]
    fn go(
        &self,
        url: &str,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
        javascript: Option<String>,
    ) -> PyResult<Page> {
        fetch_page(
            &self.config,
            FetchRequest {
                url,
                timeout,
                settle_ms,
                user_agent,
                mode: FetchMode::Content { javascript },
            },
        )
    }

    #[pyo3(signature = (url, *, full_page = true, timeout = None, settle_ms = None, user_agent = None))]
    fn screenshot(
        &self,
        url: &str,
        full_page: bool,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
    ) -> PyResult<Page> {
        fetch_page(
            &self.config,
            FetchRequest {
                url,
                timeout,
                settle_ms,
                user_agent,
                mode: FetchMode::Screenshot { full_page },
            },
        )
    }

    #[pyo3(signature = (url, *, timeout = None, settle_ms = None, user_agent = None))]
    fn markdown(
        &self,
        url: &str,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
    ) -> PyResult<String> {
        let page = self.go(url, timeout, settle_ms, user_agent, None)?;
        page.markdown(url, None)
    }

    #[pyo3(signature = (url, *, timeout = None, settle_ms = None, user_agent = None))]
    fn text(
        &self,
        url: &str,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
    ) -> PyResult<String> {
        Ok(self
            .go(url, timeout, settle_ms, user_agent, None)?
            .inner_text)
    }

    #[pyo3(signature = (url, *, timeout = None, settle_ms = None, user_agent = None))]
    fn extract_json(
        &self,
        url: &str,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
    ) -> PyResult<String> {
        let page = self.go(url, timeout, settle_ms, user_agent, None)?;
        page.extract_json(url, None)
    }

    fn __repr__(&self) -> String {
        format!(
            "Browser(timeout={}, settle_ms={}, allow_private_addresses={})",
            self.config.timeout, self.config.settle_ms, self.config.allow_private_addresses
        )
    }
}

#[pyclass(module = "servofetch", frozen, skip_from_py_object)]
#[derive(Clone)]
struct AsyncBrowser {
    config: BrowserConfig,
}

#[pymethods]
impl AsyncBrowser {
    #[new]
    #[pyo3(signature = (timeout = 30.0, settle_ms = 0, user_agent = None, allow_private_addresses = false))]
    fn new(
        timeout: f64,
        settle_ms: u64,
        user_agent: Option<String>,
        allow_private_addresses: bool,
    ) -> PyResult<Self> {
        ensure_network_policy(allow_private_addresses)?;
        validate_timeout(timeout)?;
        Ok(Self {
            config: BrowserConfig {
                timeout,
                settle_ms,
                user_agent,
                allow_private_addresses,
            },
        })
    }

    #[getter]
    fn timeout(&self) -> f64 {
        self.config.timeout
    }

    #[getter]
    fn settle_ms(&self) -> u64 {
        self.config.settle_ms
    }

    #[getter]
    fn user_agent(&self) -> Option<String> {
        self.config.user_agent.clone()
    }

    #[getter]
    fn allow_private_addresses(&self) -> bool {
        self.config.allow_private_addresses
    }

    #[pyo3(signature = (url, *, timeout = None, settle_ms = None, user_agent = None, javascript = None))]
    fn go<'py>(
        &self,
        py: Python<'py>,
        url: String,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
        javascript: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let config = self.config.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            tokio::task::spawn_blocking(move || {
                fetch_page(
                    &config,
                    FetchRequest {
                        url: &url,
                        timeout,
                        settle_ms,
                        user_agent,
                        mode: FetchMode::Content { javascript },
                    },
                )
            })
            .await
            .map_err(|err| PyRuntimeError::new_err(err.to_string()))?
        })
    }

    #[pyo3(signature = (url, *, full_page = true, timeout = None, settle_ms = None, user_agent = None))]
    fn screenshot<'py>(
        &self,
        py: Python<'py>,
        url: String,
        full_page: bool,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let config = self.config.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            tokio::task::spawn_blocking(move || {
                fetch_page(
                    &config,
                    FetchRequest {
                        url: &url,
                        timeout,
                        settle_ms,
                        user_agent,
                        mode: FetchMode::Screenshot { full_page },
                    },
                )
            })
            .await
            .map_err(|err| PyRuntimeError::new_err(err.to_string()))?
        })
    }

    #[pyo3(signature = (url, *, timeout = None, settle_ms = None, user_agent = None))]
    fn markdown<'py>(
        &self,
        py: Python<'py>,
        url: String,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let config = self.config.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            tokio::task::spawn_blocking(move || {
                let page = fetch_page(
                    &config,
                    FetchRequest {
                        url: &url,
                        timeout,
                        settle_ms,
                        user_agent,
                        mode: FetchMode::Content { javascript: None },
                    },
                )?;
                page.markdown(&url, None)
            })
            .await
            .map_err(|err| PyRuntimeError::new_err(err.to_string()))?
        })
    }

    #[pyo3(signature = (url, *, timeout = None, settle_ms = None, user_agent = None))]
    fn text<'py>(
        &self,
        py: Python<'py>,
        url: String,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let config = self.config.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            tokio::task::spawn_blocking(move || {
                Ok(fetch_page(
                    &config,
                    FetchRequest {
                        url: &url,
                        timeout,
                        settle_ms,
                        user_agent,
                        mode: FetchMode::Content { javascript: None },
                    },
                )?
                .inner_text)
            })
            .await
            .map_err(|err| PyRuntimeError::new_err(err.to_string()))?
        })
    }

    #[pyo3(signature = (url, *, timeout = None, settle_ms = None, user_agent = None))]
    fn extract_json<'py>(
        &self,
        py: Python<'py>,
        url: String,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let config = self.config.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            tokio::task::spawn_blocking(move || {
                let page = fetch_page(
                    &config,
                    FetchRequest {
                        url: &url,
                        timeout,
                        settle_ms,
                        user_agent,
                        mode: FetchMode::Content { javascript: None },
                    },
                )?;
                page.extract_json(&url, None)
            })
            .await
            .map_err(|err| PyRuntimeError::new_err(err.to_string()))?
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "AsyncBrowser(timeout={}, settle_ms={}, allow_private_addresses={})",
            self.config.timeout, self.config.settle_ms, self.config.allow_private_addresses
        )
    }
}

struct FetchRequest<'a> {
    url: &'a str,
    timeout: Option<f64>,
    settle_ms: Option<u64>,
    user_agent: Option<String>,
    mode: FetchMode,
}

enum FetchMode {
    Content { javascript: Option<String> },
    Screenshot { full_page: bool },
}

fn fetch_page(config: &BrowserConfig, request: FetchRequest<'_>) -> PyResult<Page> {
    let timeout = request.timeout.unwrap_or(config.timeout);
    validate_timeout(timeout)?;

    let mut options = match request.mode {
        FetchMode::Content {
            javascript: Some(expression),
        } => servo_fetch::FetchOptions::javascript(request.url, expression),
        FetchMode::Content { javascript: None } => servo_fetch::FetchOptions::new(request.url),
        FetchMode::Screenshot { full_page } => {
            servo_fetch::FetchOptions::screenshot(request.url, full_page)
        }
    }
    .timeout(Duration::from_secs_f64(timeout))
    .settle(Duration::from_millis(
        request.settle_ms.unwrap_or(config.settle_ms),
    ));

    if let Some(user_agent) = request.user_agent.or_else(|| config.user_agent.clone()) {
        options = options.user_agent(user_agent);
    }

    servo_fetch::fetch(options)
        .map(Page::from_servo)
        .map_err(map_error)
}

fn ensure_network_policy(allow_private_addresses: bool) -> PyResult<()> {
    let _guard = INIT_LOCK
        .lock()
        .map_err(|_| PyRuntimeError::new_err("servo-fetch network policy lock was poisoned"))?;

    match NETWORK_POLICY.get() {
        Some(existing) if *existing == allow_private_addresses => Ok(()),
        Some(_) => Err(PyValueError::new_err(
            "servo-fetch network policy is process-wide and was already initialized with a different allow_private_addresses value",
        )),
        None => {
            let policy = if allow_private_addresses {
                servo_fetch::NetworkPolicy::PERMISSIVE
            } else {
                servo_fetch::NetworkPolicy::STRICT
            };
            servo_fetch::init(policy);
            NETWORK_POLICY.set(allow_private_addresses).map_err(|_| {
                PyValueError::new_err("servo-fetch network policy was already initialized")
            })?;
            Ok(())
        }
    }
}

fn validate_timeout(timeout: f64) -> PyResult<()> {
    if timeout.is_finite() && timeout > 0.0 && timeout <= u64::MAX as f64 {
        Ok(())
    } else {
        Err(PyValueError::new_err(
            "timeout must be a positive finite number within u64 seconds",
        ))
    }
}

fn map_error(error: servo_fetch::Error) -> PyErr {
    match error {
        servo_fetch::Error::InvalidUrl { .. } => PyValueError::new_err(error.to_string()),
        servo_fetch::Error::Timeout { .. } => PyTimeoutError::new_err(error.to_string()),
        servo_fetch::Error::AddressNotAllowed(_) => PyPermissionError::new_err(error.to_string()),
        servo_fetch::Error::Io(_) => PyOSError::new_err(error.to_string()),
        _ => PyRuntimeError::new_err(error.to_string()),
    }
}

fn map_extract_error(error: servo_fetch::extract::ExtractError) -> PyErr {
    PyRuntimeError::new_err(error.to_string())
}

#[pymodule]
fn servofetch(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Browser>()?;
    m.add_class::<AsyncBrowser>()?;
    m.add_class::<Page>()?;
    m.add_class::<ConsoleMessage>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
