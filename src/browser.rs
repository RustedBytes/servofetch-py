use std::time::Duration;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use crate::errors::{ensure_network_policy, map_error, validate_timeout};
use crate::page::{Page, write_screenshot_file};
use crate::results::{CrawlResult, MappedUrl};

#[derive(Clone)]
pub(crate) struct BrowserConfig {
    timeout: f64,
    settle_ms: u64,
    user_agent: Option<String>,
    allow_private_addresses: bool,
}

#[pyclass(module = "servofetch", frozen, skip_from_py_object)]
#[derive(Clone)]
pub(crate) struct Browser {
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

    #[pyo3(signature = (url, *, timeout = None, settle_ms = None, user_agent = None, javascript = None))]
    fn fetch(
        &self,
        url: &str,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
        javascript: Option<String>,
    ) -> PyResult<Page> {
        self.go(url, timeout, settle_ms, user_agent, javascript)
    }

    #[pyo3(signature = (url, *, full_page = true, timeout = None, settle_ms = None, user_agent = None, filename = None))]
    fn screenshot(
        &self,
        url: &str,
        full_page: bool,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
        filename: Option<String>,
    ) -> PyResult<Page> {
        let page = fetch_page(
            &self.config,
            FetchRequest {
                url,
                timeout,
                settle_ms,
                user_agent,
                mode: FetchMode::Screenshot { full_page },
            },
        )?;
        write_screenshot_file(&page, filename.as_deref())?;
        Ok(page)
    }

    #[pyo3(signature = (url, *, selector = None, timeout = None, settle_ms = None, user_agent = None, javascript = None))]
    fn markdown(
        &self,
        url: &str,
        selector: Option<&str>,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
        javascript: Option<String>,
    ) -> PyResult<String> {
        let page = self.go(url, timeout, settle_ms, user_agent, javascript)?;
        page.markdown_with_options(None, selector)
    }

    #[pyo3(signature = (url, *, timeout = None, settle_ms = None, user_agent = None, javascript = None))]
    fn text(
        &self,
        url: &str,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
        javascript: Option<String>,
    ) -> PyResult<String> {
        Ok(self
            .go(url, timeout, settle_ms, user_agent, javascript)?
            .inner_text)
    }

    #[pyo3(signature = (url, *, selector = None, timeout = None, settle_ms = None, user_agent = None, javascript = None))]
    fn extract_json(
        &self,
        url: &str,
        selector: Option<&str>,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
        javascript: Option<String>,
    ) -> PyResult<String> {
        let page = self.go(url, timeout, settle_ms, user_agent, javascript)?;
        page.json_with_options(None, selector)
    }

    #[pyo3(signature = (url, *, limit = 5000, include = None, exclude = None, timeout = None, user_agent = None, no_fallback = false))]
    fn map(
        &self,
        url: &str,
        limit: usize,
        include: Option<Vec<String>>,
        exclude: Option<Vec<String>>,
        timeout: Option<f64>,
        user_agent: Option<String>,
        no_fallback: bool,
    ) -> PyResult<Vec<MappedUrl>> {
        map_urls(
            url,
            limit,
            include,
            exclude,
            timeout.unwrap_or(self.config.timeout),
            user_agent.or_else(|| self.config.user_agent.clone()),
            no_fallback,
        )
    }

    #[pyo3(signature = (url, *, limit = 50, max_depth = 3, selector = None, json = false, include = None, exclude = None, timeout = None, settle_ms = None, user_agent = None, concurrency = 1, delay_ms = None))]
    fn crawl(
        &self,
        url: &str,
        limit: usize,
        max_depth: usize,
        selector: Option<String>,
        json: bool,
        include: Option<Vec<String>>,
        exclude: Option<Vec<String>>,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
        concurrency: usize,
        delay_ms: Option<u64>,
    ) -> PyResult<Vec<CrawlResult>> {
        crawl_site(CrawlRequest {
            url,
            limit,
            max_depth,
            selector,
            json,
            include,
            exclude,
            timeout: timeout.unwrap_or(self.config.timeout),
            settle_ms: settle_ms.unwrap_or(self.config.settle_ms),
            user_agent: user_agent.or_else(|| self.config.user_agent.clone()),
            concurrency,
            delay_ms,
        })
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
pub(crate) struct AsyncBrowser {
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

    #[pyo3(signature = (url, *, timeout = None, settle_ms = None, user_agent = None, javascript = None))]
    fn fetch<'py>(
        &self,
        py: Python<'py>,
        url: String,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
        javascript: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.go(py, url, timeout, settle_ms, user_agent, javascript)
    }

    #[pyo3(signature = (url, *, full_page = true, timeout = None, settle_ms = None, user_agent = None, filename = None))]
    fn screenshot<'py>(
        &self,
        py: Python<'py>,
        url: String,
        full_page: bool,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
        filename: Option<String>,
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
                        mode: FetchMode::Screenshot { full_page },
                    },
                )?;
                write_screenshot_file(&page, filename.as_deref())?;
                Ok(page)
            })
            .await
            .map_err(|err| PyRuntimeError::new_err(err.to_string()))?
        })
    }

    #[pyo3(signature = (url, *, selector = None, timeout = None, settle_ms = None, user_agent = None, javascript = None))]
    fn markdown<'py>(
        &self,
        py: Python<'py>,
        url: String,
        selector: Option<String>,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
        javascript: Option<String>,
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
                        mode: FetchMode::Content { javascript },
                    },
                )?;
                page.markdown_with_options(None, selector.as_deref())
            })
            .await
            .map_err(|err| PyRuntimeError::new_err(err.to_string()))?
        })
    }

    #[pyo3(signature = (url, *, timeout = None, settle_ms = None, user_agent = None, javascript = None))]
    fn text<'py>(
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
                Ok(fetch_page(
                    &config,
                    FetchRequest {
                        url: &url,
                        timeout,
                        settle_ms,
                        user_agent,
                        mode: FetchMode::Content { javascript },
                    },
                )?
                .inner_text)
            })
            .await
            .map_err(|err| PyRuntimeError::new_err(err.to_string()))?
        })
    }

    #[pyo3(signature = (url, *, selector = None, timeout = None, settle_ms = None, user_agent = None, javascript = None))]
    fn extract_json<'py>(
        &self,
        py: Python<'py>,
        url: String,
        selector: Option<String>,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
        javascript: Option<String>,
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
                        mode: FetchMode::Content { javascript },
                    },
                )?;
                page.json_with_options(None, selector.as_deref())
            })
            .await
            .map_err(|err| PyRuntimeError::new_err(err.to_string()))?
        })
    }

    #[pyo3(signature = (url, *, limit = 5000, include = None, exclude = None, timeout = None, user_agent = None, no_fallback = false))]
    fn map<'py>(
        &self,
        py: Python<'py>,
        url: String,
        limit: usize,
        include: Option<Vec<String>>,
        exclude: Option<Vec<String>>,
        timeout: Option<f64>,
        user_agent: Option<String>,
        no_fallback: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let config = self.config.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            tokio::task::spawn_blocking(move || {
                map_urls(
                    &url,
                    limit,
                    include,
                    exclude,
                    timeout.unwrap_or(config.timeout),
                    user_agent.or_else(|| config.user_agent.clone()),
                    no_fallback,
                )
            })
            .await
            .map_err(|err| PyRuntimeError::new_err(err.to_string()))?
        })
    }

    #[pyo3(signature = (url, *, limit = 50, max_depth = 3, selector = None, json = false, include = None, exclude = None, timeout = None, settle_ms = None, user_agent = None, concurrency = 1, delay_ms = None))]
    fn crawl<'py>(
        &self,
        py: Python<'py>,
        url: String,
        limit: usize,
        max_depth: usize,
        selector: Option<String>,
        json: bool,
        include: Option<Vec<String>>,
        exclude: Option<Vec<String>>,
        timeout: Option<f64>,
        settle_ms: Option<u64>,
        user_agent: Option<String>,
        concurrency: usize,
        delay_ms: Option<u64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let config = self.config.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            tokio::task::spawn_blocking(move || {
                crawl_site(CrawlRequest {
                    url: &url,
                    limit,
                    max_depth,
                    selector,
                    json,
                    include,
                    exclude,
                    timeout: timeout.unwrap_or(config.timeout),
                    settle_ms: settle_ms.unwrap_or(config.settle_ms),
                    user_agent: user_agent.or_else(|| config.user_agent.clone()),
                    concurrency,
                    delay_ms,
                })
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
        .map(|page| Page::from_servo(page, request.url.to_string()))
        .map_err(map_error)
}

fn map_urls(
    url: &str,
    limit: usize,
    include: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
    timeout: f64,
    user_agent: Option<String>,
    no_fallback: bool,
) -> PyResult<Vec<MappedUrl>> {
    validate_timeout(timeout)?;
    let include_refs = include
        .as_ref()
        .map(|patterns| patterns.iter().map(String::as_str).collect::<Vec<_>>());
    let exclude_refs = exclude
        .as_ref()
        .map(|patterns| patterns.iter().map(String::as_str).collect::<Vec<_>>());

    let mut options = servo_fetch::MapOptions::new(url)
        .limit(limit)
        .timeout(timeout.ceil() as u64)
        .no_fallback(no_fallback);

    if let Some(patterns) = include_refs.as_deref() {
        options = options.include(patterns);
    }
    if let Some(patterns) = exclude_refs.as_deref() {
        options = options.exclude(patterns);
    }
    if let Some(user_agent) = user_agent {
        options = options.user_agent(user_agent);
    }

    servo_fetch::map(options)
        .map(|results| results.into_iter().map(MappedUrl::from).collect())
        .map_err(map_error)
}

struct CrawlRequest<'a> {
    url: &'a str,
    limit: usize,
    max_depth: usize,
    selector: Option<String>,
    json: bool,
    include: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
    timeout: f64,
    settle_ms: u64,
    user_agent: Option<String>,
    concurrency: usize,
    delay_ms: Option<u64>,
}

fn crawl_site(request: CrawlRequest<'_>) -> PyResult<Vec<CrawlResult>> {
    validate_timeout(request.timeout)?;
    let include_refs = request
        .include
        .as_ref()
        .map(|patterns| patterns.iter().map(String::as_str).collect::<Vec<_>>());
    let exclude_refs = request
        .exclude
        .as_ref()
        .map(|patterns| patterns.iter().map(String::as_str).collect::<Vec<_>>());

    let mut options = servo_fetch::CrawlOptions::new(request.url)
        .limit(request.limit)
        .max_depth(request.max_depth)
        .json(request.json)
        .timeout(Duration::from_secs_f64(request.timeout))
        .settle(Duration::from_millis(request.settle_ms))
        .concurrency(request.concurrency);

    if let Some(delay_ms) = request.delay_ms {
        options = options.delay(Some(Duration::from_millis(delay_ms)));
    }
    if let Some(selector) = request.selector {
        options = options.selector(selector);
    }
    if let Some(patterns) = include_refs.as_deref() {
        options = options.include(patterns);
    }
    if let Some(patterns) = exclude_refs.as_deref() {
        options = options.exclude(patterns);
    }
    if let Some(user_agent) = request.user_agent {
        options = options.user_agent(user_agent);
    }

    servo_fetch::crawl(options)
        .map(|results| results.into_iter().map(CrawlResult::from).collect())
        .map_err(map_error)
}
