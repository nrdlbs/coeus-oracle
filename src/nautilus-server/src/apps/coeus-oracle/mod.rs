// Copyright (c), Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::common::IntentMessage;
use crate::common::{to_signed_response, IntentScope, ProcessedDataResponse};
use crate::AppState;
use crate::EnclaveError;
use axum::extract::State;
use axum::Json;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use sui_rpc::field::{FieldMask, FieldMaskUtil};
use sui_rpc::proto::sui::rpc::v2::GetObjectRequest;
use sui_sdk_types::Address;

/// ====
/// Core Nautilus server logic, replace it with your own
/// relavant structs and process_data endpoint.
/// ====
/// Inner type T for IntentMessage<T>
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpdateOracleResponse {
    pub result: ResultValue,
}

/// Inner type T for ProcessDataRequest<T>
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateOracleRequest {
    feed_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ResultValue {
    STRING(String),
    BOOLEAN(bool),
    NUMBER(u64),
    VECTOR(Vec<u8>),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Payload {
    pub intent_scope: u8,
    pub timestamp_ms: u64,
    pub result: ResultValue,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum CodeExtension {
    PYTHON,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ReturnType {
    STRING,
    BOOLEAN,
    NUMBER,
    VECTOR,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OracleFeed {
    pub id: Address,
    pub blob_id: String,
    pub extension: CodeExtension,
    pub result: Option<ResultValue>,
    pub return_type: ReturnType,
    pub allow_update_timestamp_ms: u64,
}

/// Execute Python code using pyo3 and convert to expected return type
/// Returns ResultValue converted to the type specified in the oracle feed
fn execute_python_code(
    code: &str,
    expected_type: &ReturnType,
) -> Result<Option<ResultValue>, EnclaveError> {
    Python::with_gil(|py| {
        let locals = PyDict::new_bound(py);

        // Normalize user code: remove common leading whitespace (dedent), then indent for function body
        let lines: Vec<&str> = code.lines().collect();

        // Find the minimum indentation (excluding empty lines and comment-only lines)
        let min_indent = lines
            .iter()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() && !trimmed.starts_with('#')
            })
            .map(|line| line.chars().take_while(|c| c.is_whitespace()).count())
            .min()
            .unwrap_or(0);

        // Dedent all lines by the minimum indentation, then add proper indentation for function body
        let indented_code: String = lines
            .iter()
            .map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    // Keep empty lines and comments as-is (will be stripped later)
                    line.to_string()
                } else {
                    // Remove the common leading whitespace
                    let dedented = if line.len() > min_indent {
                        &line[min_indent..]
                    } else {
                        line
                    };
                    // Add 8 spaces for function body indentation
                    format!("        {}", dedented)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Strip leading empty lines and comment-only lines to ensure function body starts with actual code
        // This prevents IndentationError when user code starts with comments or blank lines
        let indented_code: String = indented_code
            .lines()
            .skip_while(|line| {
                let trimmed = line.trim();
                trimmed.is_empty() || trimmed.starts_with('#')
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Create a wrapper that captures return value and detects type
        let capture_code = format!(
            r#"
import sys
from io import StringIO

# Capture stdout
old_stdout = sys.stdout
sys.stdout = captured_output = StringIO()

try:
    # Wrap user code in a function to capture return value
    def user_code():
{}

    # Execute the function and get return value
    return_value = user_code()
    
    # Get captured stdout (before restoring)
    stdout_output = captured_output.getvalue().strip()
    
    # Determine result type
    # Check if there's an explicit return value (not None)
    if return_value is not None:
        # Has explicit return value - use it
        result_value = return_value
        result_type = type(return_value).__name__
    elif stdout_output:
        # No explicit return but has stdout from print
        # Try to detect the type from the string output
        stdout_stripped = stdout_output.strip()

        # Try to parse as integer
        try:
            result_value = int(stdout_stripped)
            result_type = "int"
        except ValueError:
            # Try to parse as float
            try:
                result_value = float(stdout_stripped)
                result_type = "float"
            except ValueError:
                # Try to parse as boolean
                if stdout_stripped.lower() in ('true', 'false'):
                    result_value = stdout_stripped.lower() == 'true'
                    result_type = "bool"
                else:
                    # Keep as string
                    result_value = stdout_output
                    result_type = "str"
    else:
        # No return and no stdout - empty result
        result_value = None
        result_type = "NoneType"
    
except Exception as e:
    result_value = f"Error: {{e}}"
    result_type = "str"
finally:
    sys.stdout = old_stdout
"#,
            indented_code
        );

        match py.run_bound(&capture_code, None, Some(&locals)) {
            Ok(_) => {
                // Get the result value from Python execution
                let result_value = match locals.get_item("result_value") {
                    Ok(Some(val)) => val,
                    Ok(None) => return Ok(None),
                    Err(e) => {
                        return Err(EnclaveError::GenericError(format!(
                            "Failed to get result_value: {}",
                            e.to_string()
                        )))
                    }
                };

                // Convert Python value to ResultValue based on EXPECTED type from contract
                match expected_type {
                    ReturnType::STRING => {
                        let s = result_value.to_string();
                        Ok(Some(ResultValue::STRING(s.trim().to_string())))
                    }
                    ReturnType::NUMBER => {
                        // Try to extract as i64 first
                        match result_value.extract::<i64>() {
                            Ok(num) if num >= 0 => Ok(Some(ResultValue::NUMBER(num as u64))),
                            Ok(num) => Err(EnclaveError::GenericError(format!(
                                "Negative number not supported: {}",
                                num
                            ))),
                            Err(_) => {
                                // Try as float and convert
                                match result_value.extract::<f64>() {
                                    Ok(f) if f >= 0.0 => Ok(Some(ResultValue::NUMBER(f as u64))),
                                    Err(_) => {
                                        // Try parsing as string
                                        let s = result_value.to_string();
                                        s.trim()
                                            .parse::<u64>()
                                            .map(|n| Some(ResultValue::NUMBER(n)))
                                            .map_err(|_| {
                                                EnclaveError::GenericError(
                                                    "Cannot convert to NUMBER".to_string(),
                                                )
                                            })
                                    }
                                    Ok(f) => Err(EnclaveError::GenericError(format!(
                                        "Negative number not supported: {}",
                                        f
                                    ))),
                                }
                            }
                        }
                    }
                    ReturnType::BOOLEAN => match result_value.extract::<bool>() {
                        Ok(b) => Ok(Some(ResultValue::BOOLEAN(b))),
                        Err(_) => {
                            // Try parsing string
                            let s = result_value.to_string().to_lowercase();
                            match s.trim() {
                                "true" | "1" => Ok(Some(ResultValue::BOOLEAN(true))),
                                "false" | "0" => Ok(Some(ResultValue::BOOLEAN(false))),
                                _ => Err(EnclaveError::GenericError(
                                    "Cannot convert to BOOLEAN".to_string(),
                                )),
                            }
                        }
                    },
                    ReturnType::VECTOR => {
                        // Try to extract as list of u8
                        match result_value.extract::<Vec<i64>>() {
                            Ok(vec) => {
                                let u8_vec: Result<Vec<u8>, _> = vec
                                    .into_iter()
                                    .map(|x| {
                                        if x >= 0 && x <= 255 {
                                            Ok(x as u8)
                                        } else {
                                            Err(EnclaveError::GenericError(format!(
                                                "Value {} out of u8 range",
                                                x
                                            )))
                                        }
                                    })
                                    .collect();
                                match u8_vec {
                                    Ok(v) => Ok(Some(ResultValue::VECTOR(v))),
                                    Err(e) => Err(e),
                                }
                            }
                            Err(_) => {
                                // Try as list of strings or other types - convert to bytes
                                match result_value.extract::<Vec<String>>() {
                                    Ok(vec) => {
                                        let bytes: Vec<u8> = vec.join("").into_bytes();
                                        Ok(Some(ResultValue::VECTOR(bytes)))
                                    }
                                    Err(_) => Err(EnclaveError::GenericError(
                                        "Unsupported list type".to_string(),
                                    )),
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => Err(EnclaveError::GenericError(format!(
                "Python execution error: {}",
                e.to_string()
            ))),
        }
    })
}

pub async fn process_data(
    State(state): State<Arc<AppState>>,
    Json(request): Json<UpdateOracleRequest>,
) -> Result<Json<ProcessedDataResponse<IntentMessage<UpdateOracleResponse>>>, EnclaveError> {
    // Clone the client to get mutable access (Client implements Clone)
    let mut sui_client = state.sui_client.clone();
    let feed_id = Address::from_hex(&request.feed_id)
        .map_err(|e| EnclaveError::GenericError(format!("Invalid feed_id format: {}", e)))?;
    println!("feed id: {:?}", feed_id);

    // Use batch_get_objects as get_object may not be available on testnet nodes
    // Create a single-object batch request
    let response = sui_client
        .ledger_client()
        .get_object(GetObjectRequest::new(&feed_id).with_read_mask(FieldMask::from_str("bcs")))
        .await
        .unwrap()
        .into_inner();

    let bcs_bytes = response
        .object
        .and_then(|obj| obj.bcs)
        .and_then(|bcs| bcs.value)
        .map(|bytes| bytes.to_vec())
        .ok_or_else(|| EnclaveError::GenericError("No BCS data in Committee object".to_string()))
        .unwrap();

    let obj: sui_sdk_types::Object = bcs::from_bytes(&bcs_bytes)
        .map_err(|e| EnclaveError::GenericError(format!("Failed to deserialize object: {}", e)))?;
    let move_object = obj
        .as_struct()
        .ok_or_else(|| EnclaveError::GenericError("Object is not a Move object".to_string()))?;
    let oracle_feed: OracleFeed = bcs::from_bytes(move_object.contents()).map_err(|e| {
        EnclaveError::GenericError(format!("Failed to deserialize OracleFeed: {}", e))
    })?;
    // Get current timestamp
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| EnclaveError::GenericError(format!("Failed to get current timestamp: {}", e)))?
        .as_millis() as u64;

    let url = Url::parse(&format!(
        "https://aggregator.walrus-testnet.walrus.space/v1/blobs/{}",
        oracle_feed.blob_id
    ))
    .unwrap();
    let response = reqwest::get(url).await.unwrap();
    let body = response.text().await.unwrap();
    println!("body: {:?}", body);

    // Execute Python code if the extension is PYTHON
    let python_result = if oracle_feed.extension == CodeExtension::PYTHON {
        // Use native pyo3 execution (embedded Python interpreter)
        execute_python_code(&body, &oracle_feed.return_type).map_err(|e| {
            EnclaveError::GenericError(format!("Failed to execute Python code: {}", e))
        })?
    } else {
        return Err(EnclaveError::GenericError(
            "Unsupported code extension".to_string(),
        ));
    };

    // Create response with detected result type
    let result = python_result.ok_or_else(|| {
        EnclaveError::GenericError("Python code execution returned no result".to_string())
    })?;
    let update_oracle_response = UpdateOracleResponse { result };

    Ok(Json(to_signed_response(
        &state.eph_kp,
        update_oracle_response,
        timestamp_ms,
        IntentScope::ProcessData,
    )))
}

// #[cfg(test)]
// mod test {
//     use super::*;
//     use crate::common::IntentMessage;
//     use axum::{extract::State, Json};
//     use fastcrypto::{ed25519::Ed25519KeyPair, traits::KeyPair};

//     #[tokio::test]
//     async fn test_process_data() {
//         use sui_rpc::Client;
//         let state = Arc::new(AppState {
//             eph_kp: Ed25519KeyPair::generate(&mut rand::thread_rng()),
//             sui_client: Client::new(Client::TESTNET_FULLNODE).unwrap(),
//         });
//         let signed_weather_response: Json<ProcessedDataResponse<IntentMessage<WeatherResponse>>> = process_data(
//             State(state),
//             Json(ProcessDataRequest {
//                 payload: UpdateOracleRequest {
//                     feed_id: "test_feed_id".to_string(),
//                 },
//             }),
//         )
//         .await
//         .unwrap();
//         assert_eq!(signed_weather_response.response.data.location, "Unknown");
//     }

//     #[test]
//     fn test_serde() {
//         // test result should be consistent with test_serde in `move/enclave/sources/enclave.move`.
//         use fastcrypto::encoding::{Encoding, Hex};
//         let payload = WeatherResponse {
//             location: "San Francisco".to_string(),
//             temperature: 13,
//         };
//         let timestamp = 1744038900000;
//         let intent_msg = IntentMessage::new(payload, timestamp, IntentScope::ProcessData);
//         let signing_payload = bcs::to_bytes(&intent_msg).expect("should not fail");
//         assert!(
//             signing_payload
//                 == Hex::decode("0020b1d110960100000d53616e204672616e636973636f0d00000000000000")
//                     .unwrap()
//         );
//     }
// }
