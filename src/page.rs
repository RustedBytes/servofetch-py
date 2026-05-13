use std::fs;

use pyo3::exceptions::{PyOSError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use crate::errors::map_extract_error;

#[pyclass(module = "servofetch", frozen, skip_from_py_object)]
#[derive(Clone)]
pub(crate) struct ConsoleMessage {
    #[pyo3(get)]
    pub(crate) level: String,
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
pub(crate) struct Page {
    #[pyo3(get)]
    url: String,
    #[pyo3(get)]
    html: String,
    #[pyo3(get)]
    pub(crate) inner_text: String,
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
    pub(crate) screenshot_png: Option<Vec<u8>>,
}

impl Page {
    pub(crate) fn from_servo(page: servo_fetch::Page, url: String) -> Self {
        let screenshot_png = page.screenshot_png().map(<[u8]>::to_vec);
        Self {
            url,
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

    fn extract_input<'a>(&'a self, url: Option<&'a str>) -> servo_fetch::extract::ExtractInput<'a> {
        let url = url.unwrap_or(&self.url);
        servo_fetch::extract::ExtractInput::new(&self.html, url)
            .with_layout_json(self.layout_json.as_deref())
            .with_inner_text(Some(&self.inner_text))
    }

    pub(crate) fn markdown_with_options(
        &self,
        url: Option<&str>,
        selector: Option<&str>,
    ) -> PyResult<String> {
        let input = self.extract_input(url).with_selector(selector);
        servo_fetch::extract::extract_text(&input).map_err(map_extract_error)
    }

    pub(crate) fn json_with_options(
        &self,
        url: Option<&str>,
        selector: Option<&str>,
    ) -> PyResult<String> {
        let input = self.extract_input(url).with_selector(selector);
        servo_fetch::extract::extract_json(&input).map_err(map_extract_error)
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

    fn text(&self) -> &str {
        &self.inner_text
    }

    #[getter]
    fn html_len(&self) -> usize {
        self.html.len()
    }

    #[getter]
    fn text_len(&self) -> usize {
        self.inner_text.len()
    }

    #[getter]
    fn has_layout(&self) -> bool {
        self.layout_json.is_some()
    }

    #[getter]
    fn has_accessibility_tree(&self) -> bool {
        self.accessibility_tree.is_some()
    }

    #[getter]
    fn console_error_count(&self) -> usize {
        self.console_messages
            .iter()
            .filter(|message| message.level == "error")
            .count()
    }

    #[pyo3(signature = (url = None, selector = None))]
    fn markdown(&self, url: Option<&str>, selector: Option<&str>) -> PyResult<String> {
        self.markdown_with_options(url, selector)
    }

    #[pyo3(signature = (selector, url = None))]
    fn select_markdown(&self, selector: &str, url: Option<&str>) -> PyResult<String> {
        self.markdown_with_options(url, Some(selector))
    }

    #[pyo3(signature = (url = None, selector = None))]
    fn extract_json(&self, url: Option<&str>, selector: Option<&str>) -> PyResult<String> {
        self.json_with_options(url, selector)
    }

    #[pyo3(signature = (selector, url = None))]
    fn select_json(&self, selector: &str, url: Option<&str>) -> PyResult<String> {
        self.json_with_options(url, Some(selector))
    }

    #[getter]
    fn has_screenshot(&self) -> bool {
        self.screenshot_png.is_some()
    }

    #[getter]
    fn screenshot_len(&self) -> usize {
        self.screenshot_png.as_ref().map_or(0, Vec::len)
    }

    fn save_screenshot(&self, filename: &str) -> PyResult<()> {
        write_screenshot_file(self, Some(filename))
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("url", &self.url)?;
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
            "Page(url={:?}, title={:?}, html_len={}, inner_text_len={})",
            self.url,
            self.title,
            self.html.len(),
            self.inner_text.len()
        )
    }
}

pub(crate) fn write_screenshot_file(page: &Page, filename: Option<&str>) -> PyResult<()> {
    let Some(filename) = filename else {
        return Ok(());
    };

    let bytes = page
        .screenshot_png
        .as_deref()
        .ok_or_else(|| PyRuntimeError::new_err("screenshot did not produce PNG data"))?;

    fs::write(filename, bytes).map_err(|err| {
        PyOSError::new_err(format!("failed to write screenshot to {filename:?}: {err}"))
    })
}
