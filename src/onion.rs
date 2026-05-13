use std::time::Duration;

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

use crate::errors::validate_timeout;

pub(crate) const DEFAULT_BOOTSTRAP: &str = "128.31.0.39:9131";

#[derive(Clone)]
pub(crate) struct OnionConfig {
    bootstrap: String,
    consensus_file: Option<String>,
    verbose: bool,
    response_limit: usize,
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

        let timeout_ms = timeout_to_millis(default_timeout)?;
        let config = net::onion::OnionTransportConfig {
            bootstrap: bootstrap.clone(),
            consensus_file: consensus_file.clone().unwrap_or_default(),
            timeout_ms,
            verbose,
            response_limit,
        };
        net::onion::configure_transport(config).map_err(PyRuntimeError::new_err)?;

        Ok(Self {
            bootstrap,
            consensus_file,
            verbose,
            response_limit,
        })
    }

    pub(crate) fn bootstrap(&self) -> String {
        self.bootstrap.clone()
    }

    pub(crate) fn consensus_file(&self) -> Option<String> {
        self.consensus_file.clone()
    }

    pub(crate) fn verbose(&self) -> bool {
        self.verbose
    }

    pub(crate) fn response_limit(&self) -> usize {
        self.response_limit
    }
}

fn timeout_to_millis(timeout: f64) -> PyResult<i32> {
    validate_timeout(timeout)?;
    let millis = Duration::from_secs_f64(timeout).as_millis().max(1);
    if millis > i32::MAX as u128 {
        return Err(PyValueError::new_err(
            "onion timeout must be less than or equal to i32::MAX milliseconds",
        ));
    }
    Ok(millis as i32)
}
