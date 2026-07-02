//! Safe D1 result deserialization helpers.
//!
//! The `worker` crate's `D1Result::results()` calls
//! `serde_wasm_bindgen::from_value(row).unwrap()` per row, which panics
//! (aborting the WASM module) on any deserialization failure. These helpers
//! bypass that method and propagate deserialization errors as proper
//! `WorkerError`s instead.

use serde::de::DeserializeOwned;
use wasm_bindgen::JsCast;
use worker::wasm_bindgen_futures::JsFuture;
use worker::worker_sys::types::D1Result as D1ResultSys;
use worker::D1PreparedStatement;

use crate::error::WorkerError;

/// Execute a prepared statement and deserialize all rows safely.
///
/// Unlike `D1Result::results()`, this returns an `Err` instead of panicking
/// when a row cannot be deserialized.
pub async fn safe_all<T: DeserializeOwned>(
    stmt: &D1PreparedStatement,
) -> Result<Vec<T>, WorkerError> {
    let inner = stmt.inner();
    let promise = inner
        .all()
        .map_err(|e| WorkerError::Internal(format!("D1 all() failed: {e:?}")))?;
    let js_value = JsFuture::from(promise)
        .await
        .map_err(|e| WorkerError::Internal(format!("D1 all() promise rejected: {e:?}")))?;
    let d1_result = js_value.unchecked_into::<D1ResultSys>();
    let results = d1_result
        .results()
        .map_err(|e| WorkerError::Internal(format!("D1 results() failed: {e:?}")))?;

    let mut rows = Vec::new();
    if let Some(arr) = results {
        for row_js in arr.iter() {
            let row: T = worker::serde_wasm_bindgen::from_value(row_js)
                .map_err(|e| WorkerError::Internal(format!("deserialize row: {e}")))?;
            rows.push(row);
        }
    }
    Ok(rows)
}
