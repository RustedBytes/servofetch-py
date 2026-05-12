use pyo3::prelude::*;
use pyo3::types::PyDict;

#[pyclass(module = "servofetch", frozen, skip_from_py_object)]
#[derive(Clone)]
pub(crate) struct MappedUrl {
    #[pyo3(get)]
    url: String,
    #[pyo3(get)]
    lastmod: Option<String>,
}

impl From<servo_fetch::MappedUrl> for MappedUrl {
    fn from(mapped: servo_fetch::MappedUrl) -> Self {
        Self {
            url: mapped.url,
            lastmod: mapped.lastmod,
        }
    }
}

#[pymethods]
impl MappedUrl {
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("url", &self.url)?;
        dict.set_item("lastmod", &self.lastmod)?;
        Ok(dict)
    }

    fn __repr__(&self) -> String {
        format!("MappedUrl(url={:?}, lastmod={:?})", self.url, self.lastmod)
    }
}

#[pyclass(module = "servofetch", frozen, skip_from_py_object)]
#[derive(Clone)]
pub(crate) struct CrawlResult {
    #[pyo3(get)]
    url: String,
    #[pyo3(get)]
    depth: usize,
    #[pyo3(get)]
    ok: bool,
    #[pyo3(get)]
    title: Option<String>,
    #[pyo3(get)]
    content: Option<String>,
    #[pyo3(get)]
    error: Option<String>,
    #[pyo3(get)]
    links_found: usize,
}

impl From<servo_fetch::CrawlResult> for CrawlResult {
    fn from(result: servo_fetch::CrawlResult) -> Self {
        match result.outcome {
            Ok(page) => Self {
                url: result.url,
                depth: result.depth,
                ok: true,
                title: page.title,
                content: Some(page.content),
                error: None,
                links_found: page.links_found,
            },
            Err(error) => Self {
                url: result.url,
                depth: result.depth,
                ok: false,
                title: None,
                content: None,
                error: Some(error.message),
                links_found: 0,
            },
        }
    }
}

#[pymethods]
impl CrawlResult {
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("url", &self.url)?;
        dict.set_item("depth", self.depth)?;
        dict.set_item("ok", self.ok)?;
        dict.set_item("title", &self.title)?;
        dict.set_item("content", &self.content)?;
        dict.set_item("error", &self.error)?;
        dict.set_item("links_found", self.links_found)?;
        Ok(dict)
    }

    fn __repr__(&self) -> String {
        format!(
            "CrawlResult(url={:?}, depth={}, ok={}, links_found={})",
            self.url, self.depth, self.ok, self.links_found
        )
    }
}
