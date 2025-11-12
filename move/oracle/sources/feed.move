/*
/// Module: oracle
module oracle::oracle;
*/

// For Move coding conventions, see
// https://docs.sui.io/concepts/sui-move-concepts/conventions

module oracle::feed;

use enclave::enclave::Enclave;
use oracle::config::Config;
use std::string::String;
use sui::clock::Clock;

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

#[error]
const EInvalidResult: vector<u8> = b"Invalid result";

#[error]
const EInvalidReceipt: vector<u8> = b"Invalid receipt";

public enum CodeExtension has store {
    PYTHON,
}

public enum ReturnType has copy, drop, store {
    STRING,
    BOOLEAN,
    NUMBER,
    VECTOR,
}

public enum Result has copy, drop, store {
    STRING(String),
    BOOLEAN(bool),
    NUMBER(u64),
    VECTOR(vector<u8>),
}

public struct Payload has copy, drop, store {
    intent_scope: u8,
    timestamp_ms: u64,
    result: Option<Result>,
}

public struct NewOracleFeedReceipt {
    id: ID,
}

public struct OracleFeed has key, store {
    id: UID,
    blob_id: String,
    extension: CodeExtension,
    result: Option<Result>,
    return_type: ReturnType,
    allow_update_timestamp_ms: u64,
}

public fun new(
    blob_id: String,
    extension: CodeExtension,
    return_type: ReturnType,
    allow_update_timestamp_ms: u64,
    ctx: &mut TxContext,
): (OracleFeed, NewOracleFeedReceipt) {
    let feed = OracleFeed {
        id: object::new(ctx),
        blob_id,
        extension,
        result: option::none(),
        return_type,
        allow_update_timestamp_ms,
    };
    let receipt = NewOracleFeedReceipt { id: object::id(&feed) };
    (feed, receipt)
}

public fun repay(feed: OracleFeed, receipt: NewOracleFeedReceipt) {
    let NewOracleFeedReceipt { id } = receipt;
    assert!(object::id(&feed) == id, EInvalidReceipt);
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
    assert!(
        clock.timestamp_ms() - payload.timestamp_ms <= config.get_max_update_time_ms(),
        EInvalidTimestamp,
    );
    assert!(clock.timestamp_ms() >= feed.allow_update_timestamp_ms, EInvalidAllowUpdateTimestamp);
    assert!(payload.result.is_some(), EInvalidResult);
    assert!(feed.result.is_some(), EInvalidResult);
    let verify_result = enclave.verify_signature<T, Payload>(
        payload.intent_scope,
        payload.timestamp_ms,
        payload,
        signature,
    );
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
        _ => abort EInvalidCodeExtension,
    }
}

public fun construct_return_type(type_bytes: vector<u8>): ReturnType {
    match (type_bytes) {
        b"string" => ReturnType::STRING,
        b"boolean" => ReturnType::BOOLEAN,
        b"number" => ReturnType::NUMBER,
        b"vector" => ReturnType::VECTOR,
        _ => abort EInvalidReturnType,
    }
}

public fun construct_payload(intent_scope: u8, timestamp_ms: u64, result: Result): Payload {
    Payload {
        intent_scope,
        timestamp_ms,
        result: option::some(result),
    }
}

public fun get_result(feed: &OracleFeed): Option<Result> {
    feed.result
}

public fun extract_u64_result(result: Result): u64 {
    match (result) {
        Result::NUMBER(number) => number,
        _ => abort EInvalidResult,
    }
}

public fun extract_boolean_result(result: Result): bool {
    match (result) {
        Result::BOOLEAN(boolean) => boolean,
        _ => abort EInvalidResult,
    }
}

public fun extract_string_result(result: Result): String {
    match (result) {
        Result::STRING(string) => string,
        _ => abort EInvalidResult,
    }
}

public fun extract_vector_result(result: Result): vector<u8> {
    match (result) {
        Result::VECTOR(vector) => vector,
        _ => abort EInvalidResult,
    }
}

public fun is_u64_result_type(return_type: ReturnType): bool {
    return_type == ReturnType::NUMBER
}

public fun is_boolean_result_type(return_type: ReturnType): bool {
    return_type == ReturnType::BOOLEAN
}

public fun is_string_result_type(return_type: ReturnType): bool {
    return_type == ReturnType::STRING
}

public fun is_vector_result_type(return_type: ReturnType): bool {
    return_type == ReturnType::VECTOR
}

public fun return_type(feed: &OracleFeed): ReturnType {
    feed.return_type
}
