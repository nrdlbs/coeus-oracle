/*
/// Module: oracle
module oracle::oracle;
*/

// For Move coding conventions, see
// https://docs.sui.io/concepts/sui-move-concepts/conventions

module oracle::feed;

use enclave::enclave::Enclave;
use std::string::String;
use sui::clock::Clock;
use oracle::config::Config;

#[error]
const EInvalidSignature: vector<u8> = b"Invalid signature";

#[error]
const EInvalidTimestamp: vector<u8> = b"Invalid timestamp";

#[error]
const EInvalidAllowUpdateTimestamp: vector<u8> = b"Invalid allow update timestamp";

#[error]
const EInvalidCodeExtension: vector<u8> = b"Invalid code extension";

#[error]
const EInvalidReturnType: vector<u8> = b"Invalid return type";

public enum CodeExtension has store {
    PYTHON,
}

public enum ReturnType has copy, drop, store {
    STRING,
    BOOLEAN,
    NUMBER,
    VECTOR,
    EMPTY,
}

public enum Result has copy, drop, store {
    STRING(String),
    BOOLEAN(bool),
    NUMBER(u64),
    VECTOR(vector<u8>),
    EMPTY,
}

public struct Payload has copy, drop, store {
    intent_scope: u8,
    timestamp_ms: u64,
    result: Result,
}

public struct OracleFeed has key, store {
    id: UID,
    object_id: ID,
    extension: CodeExtension,
    result: Result,
    return_type: ReturnType,
    allow_update_timestamp_ms: u64,
}

public fun new(
    object_id: ID,
    extension: CodeExtension,
    return_type: ReturnType,
    allow_update_timestamp_ms: u64,
    ctx: &mut TxContext
) {
    let feed = OracleFeed {
        id: object::new(ctx),
        object_id,
        extension,
        result: Result::EMPTY,
        return_type,
        allow_update_timestamp_ms,
    };
    transfer::share_object(feed);
}

public fun submit_result<T>(
    config: &Config,
    enclave: &Enclave<T>,
    payload: Payload,
    signature: &vector<u8>,
    feed: &mut OracleFeed,
    clock: &mut Clock,
) {
    assert!(clock.timestamp_ms() - payload.timestamp_ms <= config.get_max_update_time_ms(), EInvalidTimestamp);
    assert!(clock.timestamp_ms() >= feed.allow_update_timestamp_ms, EInvalidAllowUpdateTimestamp);
    let verify_result = enclave.verify_signature<T, Payload>(payload.intent_scope, payload.timestamp_ms, payload, signature);
    assert!(verify_result, EInvalidSignature);
    feed.result = payload.result;
}

public fun construct_string_result(result: String): Result {
    Result::STRING(result)
}

public fun construct_boolean_result(result: bool): Result {
    Result::BOOLEAN(result)
}

public fun construct_number_result(result: u64): Result {
    Result::NUMBER(result)
}

public fun construct_vector_result(result: vector<u8>): Result {
    Result::VECTOR(result)
}

public fun construct_code_extension(extension: vector<u8>): CodeExtension {
    match (extension) {
        b"python" => CodeExtension::PYTHON,
        _ => abort EInvalidCodeExtension
    }
}

public fun construct_return_type(type_bytes: vector<u8>): ReturnType {
    match (type_bytes) {
        b"string" => ReturnType::STRING,
        b"boolean" => ReturnType::BOOLEAN,
        b"number" => ReturnType::NUMBER,
        b"vector" => ReturnType::VECTOR,
        b"empty" => ReturnType::EMPTY,
        _ => abort EInvalidReturnType
    }
}

public fun construct_payload(intent_scope: u8, timestamp_ms: u64, result: Result): Payload {
    Payload {
        intent_scope,
        timestamp_ms,
        result,
    }
}

