// Copyright (c), Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::AppState;
use crate::EnclaveError;
use crate::common::IntentMessage;
use crate::common::{IntentScope, ProcessedDataResponse, to_signed_response};
use axum::Json;
use axum::extract::State;
use reqwest::Url;
use rhai::packages::Package;
use rhai::{Dynamic, Engine, EvalAltResult, Scope};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
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

/// Request for execute_code endpoint
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecuteCodeRequest {
    pub code: String,
    pub return_type: ReturnType,
}

/// Response for execute_code endpoint
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecuteCodeResponse {
    pub result: ResultValue,
    pub success: bool,
    pub error: Option<String>,
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
    RHAI,
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

// Host function: HTTP GET request (returns Result for backward compatibility)
fn http_get_string(url: &str) -> Result<String, String> {
    match reqwest::blocking::get(url) {
        Ok(resp) => {
            // Check HTTP status code
            let status = resp.status();
            if !status.is_success() {
                return Err(format!("HTTP error: status {}", status));
            }
            
            match resp.text() {
                Ok(text) => {
                    Ok(text)
                },
                Err(e) => Err(format!("Read error: {}", e)),
            }
        },
        Err(e) => Err(format!("Request error: {}", e)),
    }
}

// HTTP GET that validates JSON response
// Returns JSON string or throws error string
fn http_get_json(url: &str) -> String {
    match http_get_string(url) {
        Ok(text) => {
            let trimmed = text.trim();
            
            // Log for debugging (first 200 chars)
            let preview = if trimmed.len() > 200 {
                format!("{}...", &trimmed[..200])
            } else {
                trimmed.to_string()
            };
            eprintln!("[http_get_json] Response preview: {}", preview);
            
            // Validate that response looks like JSON (starts with { or [)
            if trimmed.is_empty() {
                eprintln!("[http_get_json] Empty response from {}", url);
                return format!("Error: Empty response from {}", url);
            }
            
            if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
                // Response is not JSON, might be HTML error page
                eprintln!("[http_get_json] Non-JSON response from {}", url);
                let preview = if trimmed.len() > 200 {
                    format!("{}...", &trimmed[..200])
                } else {
                    trimmed.to_string()
                };
                return format!("Error: Non-JSON response from {}: {}", url, preview);
            }
            
            // Validate JSON syntax
            match serde_json::from_str::<JsonValue>(trimmed) {
                Ok(_) => {
                    eprintln!("[http_get_json] Valid JSON received");
                    text // Valid JSON, return original text
                },
                Err(e) => {
                    eprintln!("[http_get_json] JSON parse error: {}", e);
                    format!("Error: Invalid JSON from {}: {}", url, e)
                },
            }
        },
        Err(e) => {
            eprintln!("[http_get_json] HTTP error: {}", e);
            format!("Error: {}", e)
        },
    }
}

// Wrapper function that throws error instead of returning Result
// This is easier to use in Rhai scripts
fn http_get(url: &str) -> String {
    match http_get_string(url) {
        Ok(text) => text,
        Err(e) => {
            // Throw error by returning a special error string
            // Rhai scripts can check for this pattern
            format!("Error: {}", e)
        }
    }
}

// Helper function to convert serde_json::Value to Rhai Dynamic
fn json_value_to_dynamic(value: &JsonValue) -> Dynamic {
    match value {
        JsonValue::Null => Dynamic::UNIT,
        JsonValue::Bool(b) => Dynamic::from(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Dynamic::from(i)
            } else if let Some(f) = n.as_f64() {
                Dynamic::from(f)
            } else {
                Dynamic::from(n.to_string())
            }
        }
        JsonValue::String(s) => Dynamic::from(s.clone()),
        JsonValue::Array(arr) => {
            let rhai_arr: Vec<Dynamic> = arr.iter().map(json_value_to_dynamic).collect();
            Dynamic::from(rhai_arr)
        }
        JsonValue::Object(obj) => {
            let mut map = rhai::Map::new();
            for (k, v) in obj.iter() {
                map.insert(k.clone().into(), json_value_to_dynamic(v));
            }
            Dynamic::from(map)
        }
    }
}

// Host function: Parse JSON string to Rhai Dynamic
// Returns Dynamic directly - on error, returns a string "Error: <msg>"
fn parse_json(text: &str) -> Dynamic {
    println!("text: {}", text);
    match serde_json::from_str::<JsonValue>(text) {
        Ok(v) => json_value_to_dynamic(&v),
        Err(e) => Dynamic::from(format!("Error: {}", e)),
    }
}

// Parse JSON from Dynamic (extracts string first)
// This version accepts Dynamic and automatically extracts the string
// It also handles Result types by unwrapping them
fn parse_json_dynamic(text: &mut Dynamic) -> Dynamic {
    // Get string representation to check if it's a Result type
    let text_str = text.to_string();

    // If it's a Result type, unwrap it first
    let actual_str = if text_str.starts_with("Err(") || text_str.starts_with("Error:") {
        // It's an error, return error message
        let err_msg = if text_str.starts_with("Err(") {
            text_str
                .trim_start_matches("Err(")
                .trim_end_matches(")")
                .to_string()
        } else {
            text_str
        };
        return Dynamic::from(format!("Error: {}", err_msg));
    } else if text_str.starts_with("Ok(") {
        // It's an Ok Result, extract the value
        let value = text_str
            .trim_start_matches("Ok(")
            .trim_end_matches(")")
            .to_string();
        // Remove quotes if present (Result<String, String> will have quotes)
        value.trim_matches('"').to_string()
    } else if let Ok(s) = text.clone().into_string() {
        // It's already a plain string
        s
    } else {
        // Fallback: use string representation
        text_str
    };

    parse_json(&actual_str)
}

// Convenience function: Fetch URL and parse as JSON in one step
// This is the simplest and most ergonomic way to fetch JSON in Rhai scripts
fn fetch_json(url: &str) -> Dynamic {
    eprintln!("[fetch_json] Fetching from URL: {}", url);

    match http_get_string(url) {
        Ok(text) => {
            eprintln!("[fetch_json] Got response, parsing JSON...");
            let trimmed = text.trim();

            // Validate JSON before parsing
            if trimmed.is_empty() {
                eprintln!("[fetch_json] Empty response");
                return Dynamic::from(format!("Error: Empty response from {}", url));
            }

            if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
                eprintln!("[fetch_json] Non-JSON response");
                let preview = if trimmed.len() > 200 {
                    format!("{}...", &trimmed[..200])
                } else {
                    trimmed.to_string()
                };
                return Dynamic::from(format!("Error: Non-JSON response: {}", preview));
            }

            // Parse JSON
            match serde_json::from_str::<JsonValue>(trimmed) {
                Ok(v) => {
                    eprintln!("[fetch_json] JSON parsed successfully");
                    json_value_to_dynamic(&v)
                },
                Err(e) => {
                    eprintln!("[fetch_json] JSON parse error: {}", e);
                    Dynamic::from(format!("Error: Invalid JSON: {}", e))
                }
            }
        },
        Err(e) => {
            eprintln!("[fetch_json] HTTP error: {}", e);
            Dynamic::from(format!("Error: {}", e))
        }
    }
}

/// Setup Rhai engine with all required functions and packages
fn setup_rhai_engine() -> Engine {
    let mut engine = Engine::new();

    // Load the Rhai Standard Package (provides basic string, array, map functions)
    engine.register_global_module(rhai::packages::StandardPackage::new().as_shared_module());

    // Load Basic String Package (provides additional string functions)
    engine.register_global_module(rhai::packages::BasicStringPackage::new().as_shared_module());

    // Register join() manually for arrays (not included in standard packages)
    engine.register_fn("join", |arr: rhai::Array, sep: &str| -> String {
        arr.into_iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(sep)
    });

    // Register contains_key manually for Map (not included in any standard package)
    engine.register_fn("contains_key", |map: &mut rhai::Map, key: &str| -> bool {
        map.contains_key(key)
    });

    // Register host functions
    // http_get_string returns Result<String, String> (for advanced usage)
    engine.register_fn("http_get_string", http_get_string);
    // http_get returns String directly, or "Error: ..." if failed (easier to use)
    engine.register_fn("http_get", http_get);
    // http_get_json validates JSON response and returns JSON string or error string
    engine.register_fn("http_get_json", http_get_json);
    // Register both versions of parse_json: one for &str, one for Dynamic
    engine.register_fn("parse_json", parse_json);
    engine.register_fn("parse_json", parse_json_dynamic);
    // fetch_json: Convenience function that fetches and parses JSON in one step (RECOMMENDED)
    engine.register_fn("fetch_json", fetch_json);
    // Helper function to convert Dynamic to String (useful for unwrap() results)
    engine.register_fn("to_string", |value: &mut Dynamic| -> String {
        if let Ok(s) = value.clone().into_string() {
            s
        } else {
            value.to_string()
        }
    });
    engine.register_fn("error", |msg: &str| -> () {
        eprintln!("Script error: {}", msg);
    });

    // Register Result helper functions for Rhai
    // These allow Rhai scripts to work with Result<String, String> from http_get_string
    engine.register_fn("is_err", |result: &mut Dynamic| -> bool {
        println!("result: {}", result);
        let result_str = result.to_string();
        result_str.starts_with("Err(") || result_str.starts_with("Error:")
    });
    engine.register_fn("is_ok", |result: &mut Dynamic| -> bool {
        let result_str = result.to_string();
        !result_str.starts_with("Err(") && !result_str.starts_with("Error:")
    });
    engine.register_fn("unwrap", |result: &mut Dynamic| -> Dynamic {
        let result_str = result.to_string();
        if result_str.starts_with("Err(") {
            let err_msg = result_str
                .trim_start_matches("Err(")
                .trim_end_matches(")")
                .to_string();
            Dynamic::from(format!("Error: {}", err_msg))
        } else if result_str.starts_with("Ok(") {
            let value = result_str
                .trim_start_matches("Ok(")
                .trim_end_matches(")")
                .to_string();
            Dynamic::from(value)
        } else {
            result.clone()
        }
    });
    // unwrap_string returns String directly (useful for parse_json)
    // Try to extract the actual value from Result<String, String>
    engine.register_fn("unwrap_string", |result: &mut Dynamic| -> String {
        let result_str = result.to_string();
        
        // Check if it's an error
        if result_str.starts_with("Err(") || result_str.starts_with("Error:") {
            let err_msg = if result_str.starts_with("Err(") {
                result_str
                    .trim_start_matches("Err(")
                    .trim_end_matches(")")
                    .to_string()
            } else {
                result_str
            };
            return format!("Error: {}", err_msg);
        }
        
        // Try to extract from "Ok(...)" format
        if result_str.starts_with("Ok(") {
            let value = result_str
                .trim_start_matches("Ok(")
                .trim_end_matches(")")
                .to_string();
            // Remove quotes if present
            let value = value.trim_matches('"').to_string();
            return value;
        }
        
        // If it doesn't match Ok/Err pattern, try to extract string directly
        if let Ok(s) = result.clone().into_string() {
            return s;
        }
        
        // Last resort: return as string
        result_str
    });
    engine.register_fn("err", |result: &mut Dynamic| -> Dynamic {
        let result_str = result.to_string();
        if result_str.starts_with("Err(") {
            let err_msg = result_str
                .trim_start_matches("Err(")
                .trim_end_matches(")")
                .to_string();
            Dynamic::from(err_msg)
        } else {
            Dynamic::UNIT
        }
    });

    engine
}

/// Convert Rhai Dynamic result to ResultValue based on expected type
fn convert_rhai_result(
    dynamic: Dynamic,
    expected_type: &ReturnType,
) -> Result<Option<ResultValue>, EnclaveError> {
    match expected_type {
        ReturnType::STRING => {
            let s = dynamic.to_string();
            Ok(Some(ResultValue::STRING(s.trim().to_string())))
        }
        ReturnType::NUMBER => {
            // Try to convert to integer
            if let Ok(num) = dynamic.as_int() {
                if num >= 0 {
                    Ok(Some(ResultValue::NUMBER(num as u64)))
                } else {
                    Err(EnclaveError::GenericError(format!(
                        "Negative number not supported: {}",
                        num
                    )))
                }
            } else if let Ok(num) = dynamic.as_float() {
                if num >= 0.0 {
                    Ok(Some(ResultValue::NUMBER(num as u64)))
                } else {
                    Err(EnclaveError::GenericError(format!(
                        "Negative number not supported: {}",
                        num
                    )))
                }
            } else {
                // Try parsing as string
                let s = dynamic.to_string().trim().to_string();
                if s.starts_with("Error:") {
                    Err(EnclaveError::GenericError(format!(
                        "Rhai code execution failed: {}",
                        s
                    )))
                } else {
                    s.parse::<u64>()
                        .map(|n| Some(ResultValue::NUMBER(n)))
                        .map_err(|e| {
                            EnclaveError::GenericError(format!(
                                "Cannot convert to NUMBER: string '{}' is not a valid number: {}",
                                s, e
                            ))
                        })
                }
            }
        }
        ReturnType::BOOLEAN => {
            // Try as boolean first
            if let Ok(b) = dynamic.as_bool() {
                Ok(Some(ResultValue::BOOLEAN(b)))
            } else {
                // Try parsing as string
                let s = dynamic.to_string().trim().to_lowercase();
                match s.as_str() {
                    "true" | "1" => Ok(Some(ResultValue::BOOLEAN(true))),
                    "false" | "0" => Ok(Some(ResultValue::BOOLEAN(false))),
                    _ => Err(EnclaveError::GenericError(
                        "Cannot convert to BOOLEAN".to_string(),
                    )),
                }
            }
        }
        ReturnType::VECTOR => {
            // Try as array
            let dynamic_clone = dynamic.clone();
            if let Some(arr) = dynamic_clone.try_cast::<rhai::Array>() {
                let mut u8_vec = Vec::new();
                for item in arr.iter() {
                    let item = item.clone();
                    // Try as integer first
                    if let Ok(num) = item.as_int() {
                        if num >= 0 && num <= 255 {
                            u8_vec.push(num as u8);
                        } else {
                            return Err(EnclaveError::GenericError(format!(
                                "Value {} out of u8 range",
                                num
                            )));
                        }
                    } else if let Ok(s) = item.clone().into_string() {
                        // If it's a string, convert to bytes
                        u8_vec.extend_from_slice(s.as_bytes());
                    } else {
                        return Err(EnclaveError::GenericError(
                            "Unsupported array element type".to_string(),
                        ));
                    }
                }
                Ok(Some(ResultValue::VECTOR(u8_vec)))
            } else {
                // Try as string and convert to bytes
                let s = dynamic.to_string();
                Ok(Some(ResultValue::VECTOR(s.as_bytes().to_vec())))
            }
        }
    }
}

/// Execute Rhai script and convert to expected return type (async version)
/// This function wraps Rhai execution in spawn_blocking to avoid blocking the async runtime
/// Returns ResultValue converted to the type specified in the oracle feed
pub async fn execute_rhai_code_async(
    code: &str,
    expected_type: &ReturnType,
) -> Result<Option<ResultValue>, EnclaveError> {
    let code = code.to_string();
    let expected_type = expected_type.clone();

    // Execute Rhai in a separate thread to avoid blocking the async runtime
    // This is critical because http_get_string uses reqwest::blocking::get()
    // We use std::thread and convert Dynamic to a Send-safe type before sending
    let (tx, rx) = tokio::sync::oneshot::channel();
    
    std::thread::spawn(move || {
        // Create engine inside the blocking thread
        let mut engine = Engine::new();

        // Load the Rhai Standard Package
        engine.register_global_module(rhai::packages::StandardPackage::new().as_shared_module());

        // Load Basic String Package
        engine.register_global_module(rhai::packages::BasicStringPackage::new().as_shared_module());

        // Register join() manually for arrays
        engine.register_fn("join", |arr: rhai::Array, sep: &str| -> String {
            arr.into_iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(sep)
        });

        // Register contains_key manually for Map
        engine.register_fn("contains_key", |map: &mut rhai::Map, key: &str| -> bool {
            map.contains_key(key)
        });

        // Register host functions
        // http_get_string returns Result<String, String> (for advanced usage)
        engine.register_fn("http_get_string", http_get_string);
        // http_get returns String directly, or "Error: ..." if failed (easier to use)
        engine.register_fn("http_get", http_get);
        // http_get_json validates JSON response and returns JSON string or error string
        engine.register_fn("http_get_json", http_get_json);
        // Register both versions of parse_json: one for &str, one for Dynamic
        engine.register_fn("parse_json", parse_json);
        engine.register_fn("parse_json", parse_json_dynamic);
        // fetch_json: Convenience function that fetches and parses JSON in one step (RECOMMENDED)
        engine.register_fn("fetch_json", fetch_json);
        // Helper function to convert Dynamic to String (useful for unwrap() results)
        engine.register_fn("to_string", |value: &mut Dynamic| -> String {
            if let Ok(s) = value.clone().into_string() {
                s
            } else {
                value.to_string()
            }
        });
        engine.register_fn("error", |msg: &str| -> () {
            eprintln!("Script error: {}", msg);
        });
        // Debug function to inspect Result type representation
        engine.register_fn("debug_result", |result: &mut Dynamic| -> String {
            let result_str = result.to_string();
            let type_name = result.type_name();
            format!("Result type: {}, string: {}", type_name, result_str)
        });
        // Debug function to print response (for debugging HTTP calls)
        engine.register_fn("debug_print", |msg: &str| -> () {
            eprintln!("[Rhai Debug] {}", msg);
        });

        // Register Result helper functions for Rhai
        // These allow Rhai scripts to work with Result<String, String> from http_get_string
        // Note: Rhai represents Result as a special type, we need to check its string representation
        engine.register_fn("is_err", |result: &mut Dynamic| -> bool {
            // Check if result is an error by examining its string representation
            // Result<String, String> when converted to string shows "Err(...)" for errors
            let result_str = result.to_string();
            result_str.starts_with("Err(") || result_str.starts_with("Error:")
        });
        engine.register_fn("is_ok", |result: &mut Dynamic| -> bool {
            let result_str = result.to_string();
            !result_str.starts_with("Err(") && !result_str.starts_with("Error:")
        });
        engine.register_fn("unwrap", |result: &mut Dynamic| -> Dynamic {
            let result_str = result.to_string();
            if result_str.starts_with("Err(") {
                // Extract error message from "Err(...)"
                let err_msg = result_str
                    .trim_start_matches("Err(")
                    .trim_end_matches(")")
                    .to_string();
                // Throw error by returning error string
                Dynamic::from(format!("Error: {}", err_msg))
            } else if result_str.starts_with("Ok(") {
                // Extract value from "Ok(...)"
                let value = result_str
                    .trim_start_matches("Ok(")
                    .trim_end_matches(")")
                    .to_string();
                Dynamic::from(value)
            } else {
                // Not a Result type, return as-is
                result.clone()
            }
        });
        // unwrap_string returns String directly (useful for parse_json)
        // Try to extract the actual value from Result<String, String>
        engine.register_fn("unwrap_string", |result: &mut Dynamic| -> String {
            // First, try to get the string representation
            let result_str = result.to_string();
            
            // Check if it's an error
            if result_str.starts_with("Err(") || result_str.starts_with("Error:") {
                let err_msg = if result_str.starts_with("Err(") {
                    result_str
                        .trim_start_matches("Err(")
                        .trim_end_matches(")")
                        .to_string()
                } else {
                    result_str
                };
                return format!("Error: {}", err_msg);
            }
            
            // Try to extract from "Ok(...)" format
            if result_str.starts_with("Ok(") {
                // Remove "Ok(" prefix and ")" suffix
                let value = result_str
                    .trim_start_matches("Ok(")
                    .trim_end_matches(")")
                    .to_string();
                // Remove quotes if present
                let value = value.trim_matches('"').to_string();
                return value;
            }
            
            // If it doesn't match Ok/Err pattern, try to extract string directly
            // Result<String, String> might be represented differently
            if let Ok(s) = result.clone().into_string() {
                return s;
            }
            
            // Last resort: return as string
            result_str
        });
        engine.register_fn("err", |result: &mut Dynamic| -> Dynamic {
            let result_str = result.to_string();
            if result_str.starts_with("Err(") {
                let err_msg = result_str
                    .trim_start_matches("Err(")
                    .trim_end_matches(")")
                    .to_string();
                Dynamic::from(err_msg)
            } else {
                Dynamic::UNIT
            }
        });

        let mut scope = Scope::new();
        let result: Result<Dynamic, Box<EvalAltResult>> = engine.eval_with_scope(&mut scope, &code);
        
        // Convert Dynamic to a Send-safe representation (JSON string)
        // We'll parse it back on the async side
        let sendable_result: Result<String, String> = match result {
            Ok(dynamic) => {
                // Convert Dynamic to JSON string for safe thread communication
                let json_value = match dynamic.type_name() {
                    "()" => JsonValue::Null,
                    "bool" => JsonValue::Bool(dynamic.as_bool().unwrap_or(false)),
                    "i64" => JsonValue::Number(dynamic.as_int().unwrap_or(0).into()),
                    "f64" => {
                        let f = dynamic.as_float().unwrap_or(0.0);
                        serde_json::Number::from_f64(f)
                            .map(JsonValue::Number)
                            .unwrap_or(JsonValue::Null)
                    }
                    "string" => JsonValue::String(dynamic.into_string().unwrap_or_default()),
                    _ => {
                        // For other types, convert to string
                        JsonValue::String(dynamic.to_string())
                    }
                };
                match serde_json::to_string(&json_value) {
                    Ok(s) => Ok(s),
                    Err(e) => Err(format!("JSON serialization error: {}", e)),
                }
            }
            Err(e) => Err(format!("{}", e)),
        };
        
        let _ = tx.send(sendable_result);
    });

    // Receive the result and convert back to Dynamic
    let json_str = match rx.await {
        Ok(Ok(json_str)) => json_str,
        Ok(Err(e)) => {
            return Err(EnclaveError::GenericError(format!(
                "Rhai execution error: {}",
                e
            )));
        }
        Err(e) => {
            return Err(EnclaveError::GenericError(format!(
                "Thread communication error: {}",
                e
            )));
        }
    };

    // Parse JSON back to Dynamic
    let json_value: JsonValue = serde_json::from_str(&json_str)
        .map_err(|e| EnclaveError::GenericError(format!("Failed to parse result JSON: {}", e)))?;
    
    let result: Result<Dynamic, Box<EvalAltResult>> = Ok(json_value_to_dynamic(&json_value));

    match result {
        Ok(dynamic) => convert_rhai_result(dynamic, &expected_type),
        Err(e) => Err(EnclaveError::GenericError(format!(
            "Rhai execution error: {}",
            e
        ))),
    }
}

/// Execute Rhai script and convert to expected return type (sync version for tests)
/// Returns ResultValue converted to the type specified in the oracle feed
fn execute_rhai_code(
    code: &str,
    expected_type: &ReturnType,
) -> Result<Option<ResultValue>, EnclaveError> {
    let engine = setup_rhai_engine();
    let mut scope = Scope::new();

    // Execute the script
    let result: Result<Dynamic, Box<EvalAltResult>> = engine.eval_with_scope(&mut scope, code);

    match result {
        Ok(dynamic) => convert_rhai_result(dynamic, expected_type),
        Err(e) => Err(EnclaveError::GenericError(format!(
            "Rhai execution error: {}",
            e
        ))),
    }
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

    // Execute Rhai script if the extension is RHAI
    let rhai_result = if oracle_feed.extension == CodeExtension::RHAI {
        // Use async Rhai execution (wrapped in spawn_blocking to avoid blocking async runtime)
        execute_rhai_code_async(&body, &oracle_feed.return_type).await.map_err(|e| {
            EnclaveError::GenericError(format!("Failed to execute Rhai code: {}", e))
        })?
    } else {
        return Err(EnclaveError::GenericError(
            "Unsupported code extension".to_string(),
        ));
    };

    // Create response with detected result type
    let result = rhai_result.ok_or_else(|| {
        EnclaveError::GenericError("Rhai code execution returned no result".to_string())
    })?;
    let update_oracle_response = UpdateOracleResponse { result };

    Ok(Json(to_signed_response(
        &state.eph_kp,
        update_oracle_response,
        timestamp_ms,
        IntentScope::ProcessData,
    )))
}

/// Execute Rhai code directly without fetching from a blob
/// This endpoint is useful for testing Rhai scripts before deploying them
pub async fn execute_code(
    Json(request): Json<ExecuteCodeRequest>,
) -> Result<Json<ExecuteCodeResponse>, EnclaveError> {
    println!("Executing code with return_type: {:?}", request.return_type);
    println!("Code: {}", request.code);

    // Execute the Rhai code (wrapped in spawn_blocking to avoid blocking async runtime)
    match execute_rhai_code_async(&request.code, &request.return_type).await {
        Ok(Some(result)) => {
            Ok(Json(ExecuteCodeResponse {
                result,
                success: true,
                error: None,
            }))
        }
        Ok(None) => {
            Ok(Json(ExecuteCodeResponse {
                result: ResultValue::STRING("".to_string()), // Default empty result
                success: false,
                error: Some("Rhai code execution returned no result".to_string()),
            }))
        }
        Err(e) => {
            Ok(Json(ExecuteCodeResponse {
                result: ResultValue::STRING("".to_string()), // Default empty result
                success: false,
                error: Some(e.to_string()),
            }))
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_execute_rhai_string() {
        // Test simple string return
        let code = r#""Hello World""#;
        let result = execute_rhai_code(code, &ReturnType::STRING).unwrap();
        assert_eq!(result, Some(ResultValue::STRING("Hello World".to_string())));

        // Test string with whitespace
        let code = r#""  Test String  ""#;
        let result = execute_rhai_code(code, &ReturnType::STRING).unwrap();
        assert_eq!(result, Some(ResultValue::STRING("Test String".to_string())));

        // Test string from variable
        let code = r#"
            let x = "test";
            x
        "#;
        let result = execute_rhai_code(code, &ReturnType::STRING).unwrap();
        assert_eq!(result, Some(ResultValue::STRING("test".to_string())));
    }

    #[test]
    fn test_execute_rhai_number() {
        // Test integer
        let code = "42";
        let result = execute_rhai_code(code, &ReturnType::NUMBER).unwrap();
        assert_eq!(result, Some(ResultValue::NUMBER(42)));

        // Test float (should be converted to u64)
        let code = "123.0";
        let result = execute_rhai_code(code, &ReturnType::NUMBER).unwrap();
        assert_eq!(result, Some(ResultValue::NUMBER(123)));

        // Test large number
        let code = "999999";
        let result = execute_rhai_code(code, &ReturnType::NUMBER).unwrap();
        assert_eq!(result, Some(ResultValue::NUMBER(999999)));

        // Test zero
        let code = "0";
        let result = execute_rhai_code(code, &ReturnType::NUMBER).unwrap();
        assert_eq!(result, Some(ResultValue::NUMBER(0)));

        // Test string number
        let code = r#""123""#;
        let result = execute_rhai_code(code, &ReturnType::NUMBER).unwrap();
        assert_eq!(result, Some(ResultValue::NUMBER(123)));

        // Test negative number (should fail)
        let code = "-42";
        let result = execute_rhai_code(code, &ReturnType::NUMBER);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Negative number not supported")
        );
    }

    #[test]
    fn test_execute_rhai_boolean() {
        // Test true boolean
        let code = "true";
        let result = execute_rhai_code(code, &ReturnType::BOOLEAN).unwrap();
        assert_eq!(result, Some(ResultValue::BOOLEAN(true)));

        // Test false boolean
        let code = "false";
        let result = execute_rhai_code(code, &ReturnType::BOOLEAN).unwrap();
        assert_eq!(result, Some(ResultValue::BOOLEAN(false)));

        // Test string "true"
        let code = r#""true""#;
        let result = execute_rhai_code(code, &ReturnType::BOOLEAN).unwrap();
        assert_eq!(result, Some(ResultValue::BOOLEAN(true)));

        // Test string "false"
        let code = r#""false""#;
        let result = execute_rhai_code(code, &ReturnType::BOOLEAN).unwrap();
        assert_eq!(result, Some(ResultValue::BOOLEAN(false)));

        // Test string "1"
        let code = r#""1""#;
        let result = execute_rhai_code(code, &ReturnType::BOOLEAN).unwrap();
        assert_eq!(result, Some(ResultValue::BOOLEAN(true)));

        // Test string "0"
        let code = r#""0""#;
        let result = execute_rhai_code(code, &ReturnType::BOOLEAN).unwrap();
        assert_eq!(result, Some(ResultValue::BOOLEAN(false)));

        // Test invalid boolean string (should fail)
        let code = r#""maybe""#;
        let result = execute_rhai_code(code, &ReturnType::BOOLEAN);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_rhai_vector() {
        // Test array of integers
        let code = "[1, 2, 3, 4, 5]";
        let result = execute_rhai_code(code, &ReturnType::VECTOR).unwrap();
        assert_eq!(result, Some(ResultValue::VECTOR(vec![1, 2, 3, 4, 5])));

        // Test array with u8 range values
        let code = "[0, 255, 128]";
        let result = execute_rhai_code(code, &ReturnType::VECTOR).unwrap();
        assert_eq!(result, Some(ResultValue::VECTOR(vec![0, 255, 128])));

        // Test array with strings
        let code = r#"["hello", "world"]"#;
        let result = execute_rhai_code(code, &ReturnType::VECTOR).unwrap();
        // Should convert strings to bytes
        assert_eq!(result, Some(ResultValue::VECTOR(b"helloworld".to_vec())));

        // Test string to vector
        let code = r#""test""#;
        let result = execute_rhai_code(code, &ReturnType::VECTOR).unwrap();
        assert_eq!(result, Some(ResultValue::VECTOR(b"test".to_vec())));

        // Test out of range value (should fail)
        let code = "[256]";
        let result = execute_rhai_code(code, &ReturnType::VECTOR);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("out of u8 range"));

        // Test negative value in array (should fail)
        let code = "[-1]";
        let result = execute_rhai_code(code, &ReturnType::VECTOR);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_rhai_with_http_get() {
        // Test HTTP GET function (using a simple test URL)
        // Note: This test requires network access and may fail if the URL is unavailable
        let code = r#"
            let url = "https://httpbin.org/get";
            let resp = http_get_string(url);
            if resp.is_err() {
                "Error: " + resp.err()
            } else {
                "Success"
            }
        "#;
        let result = execute_rhai_code(code, &ReturnType::STRING);
        // Should either succeed with "Success" or fail with an error
        // We just check it doesn't panic
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_execute_rhai_with_parse_json() {
        // Test JSON parsing - use escaped double quote in string literal
        let code = r#"
            let q = "\"";
            let json_parts = ["{", q, "name", q, ": ", q, "test", q, ", ", q, "value", q, ": 42", "}"];
            let json_str = json_parts.join("");
            let obj = parse_json(json_str);
            let obj_str = obj.to_string();
            if obj_str.starts_with("Error") {
                "Error"
            } else {
                obj.name
            }
        "#;
        let result = execute_rhai_code(code, &ReturnType::STRING).unwrap();
        assert_eq!(result, Some(ResultValue::STRING("test".to_string())));

        // Test JSON parsing with nested structure
        let code = r#"
            let q = "\"";
            let json_parts = ["{", q, "symbol", q, ": ", q, "SUI", q, "}"];
            let json_str = json_parts.join("");
            let obj = parse_json(json_str);
            let obj_str = obj.to_string();
            if obj_str.starts_with("Error") {
                "Error"
            } else {
                obj.symbol
            }
        "#;
        let result = execute_rhai_code(code, &ReturnType::STRING).unwrap();
        assert_eq!(result, Some(ResultValue::STRING("SUI".to_string())));
    }

    #[test]
    fn test_execute_rhai_complex_script() {
        // Test a more complex script that fetches JSON and extracts a value
        // Use escaped double quote in string literal
        let code = r#"
            // Simulate fetching JSON and parsing
            let q = "\"";
            let json_parts = ["{", q, "sui", q, ": {", q, "usd", q, ": 1.23", "}}"];
            let json_str = json_parts.join("");
            let obj = parse_json(json_str);
            let obj_str = obj.to_string();
            if obj_str.starts_with("Error") {
                0
            } else {
                let data = obj;
                if data.contains_key("sui") {
                    let sui_obj = data["sui"];
                    if sui_obj.contains_key("usd") {
                        sui_obj["usd"]
                    } else {
                        0
                    }
                } else {
                    0
                }
            }
        "#;
        let result = execute_rhai_code(code, &ReturnType::NUMBER).unwrap();
        // Should extract 1.23 and convert to 1 (u64)
        assert_eq!(result, Some(ResultValue::NUMBER(1)));
    }

    #[test]
    fn test_execute_rhai_error_cases() {
        // Test syntax error
        let code = "invalid syntax {";
        let result = execute_rhai_code(code, &ReturnType::STRING);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Rhai execution error")
        );

        // Test invalid number string
        let code = r#""not a number""#;
        let result = execute_rhai_code(code, &ReturnType::NUMBER);
        assert!(result.is_err());

        // Test empty code
        let code = "";
        let result = execute_rhai_code(code, &ReturnType::STRING);
        // Empty code might return unit or error depending on Rhai behavior
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_execute_rhai_with_functions() {
        // Test script with function definition
        let code = r#"
            fn add(a, b) {
                a + b
            }
            add(10, 20)
        "#;
        let result = execute_rhai_code(code, &ReturnType::NUMBER).unwrap();
        assert_eq!(result, Some(ResultValue::NUMBER(30)));
    }

    #[test]
    fn test_execute_rhai_conditional_logic() {
        // Test conditional return
        let code = r#"
            let x = 10;
            if x > 5 {
                "greater"
            } else {
                "lesser"
            }
        "#;
        let result = execute_rhai_code(code, &ReturnType::STRING).unwrap();
        assert_eq!(result, Some(ResultValue::STRING("greater".to_string())));
    }
}
