use std::sync::{Mutex, OnceLock};

use pyo3::exceptions::{
    PyOSError, PyPermissionError, PyRuntimeError, PyTimeoutError, PyValueError,
};
use pyo3::prelude::*;

static NETWORK_POLICY: OnceLock<bool> = OnceLock::new();
static INIT_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn ensure_network_policy(allow_private_addresses: bool) -> PyResult<()> {
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

pub(crate) fn validate_timeout(timeout: f64) -> PyResult<()> {
    if timeout.is_finite() && timeout > 0.0 && timeout <= u64::MAX as f64 {
        Ok(())
    } else {
        Err(PyValueError::new_err(
            "timeout must be a positive finite number within u64 seconds",
        ))
    }
}

pub(crate) fn map_error(error: servo_fetch::Error) -> PyErr {
    match error {
        servo_fetch::Error::InvalidUrl { .. } => PyValueError::new_err(error.to_string()),
        servo_fetch::Error::Timeout { .. } => PyTimeoutError::new_err(error.to_string()),
        servo_fetch::Error::AddressNotAllowed(_) => PyPermissionError::new_err(error.to_string()),
        servo_fetch::Error::Io(_) => PyOSError::new_err(error.to_string()),
        _ => PyRuntimeError::new_err(error.to_string()),
    }
}

pub(crate) fn map_extract_error(error: servo_fetch::extract::ExtractError) -> PyErr {
    PyRuntimeError::new_err(error.to_string())
}
