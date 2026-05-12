mod browser;
mod errors;
mod page;
mod results;

use pyo3::prelude::*;

use browser::{AsyncBrowser, Browser};
use page::{ConsoleMessage, Page};
use results::{CrawlResult, MappedUrl};

#[pymodule]
fn servofetch(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Browser>()?;
    m.add_class::<AsyncBrowser>()?;
    m.add_class::<Page>()?;
    m.add_class::<ConsoleMessage>()?;
    m.add_class::<MappedUrl>()?;
    m.add_class::<CrawlResult>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
