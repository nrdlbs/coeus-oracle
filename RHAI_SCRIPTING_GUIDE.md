# Rhai Scripting Guide for Coeus Oracle

## Summary of Changes

The Rhai script execution system has been standardized to fix parsing issues and improve developer experience. The main improvements are:

### 1. **New `fetch_json()` Function (RECOMMENDED)**
A new convenience function that combines HTTP GET and JSON parsing in a single call:

```rhai
let data = fetch_json("https://api.example.com/data");
```

This is now the **recommended way** to fetch JSON data in oracle scripts.

### 2. **Fixed `parse_json()` Function**
The `parse_json()` function now automatically handles `Result` types from `http_get_string()`:

```rhai
// OLD (broken):
let resp = http_get_string(url);
let json = parse_json(unwrap_string(resp));  // ❌ This doesn't work!

// NEW (works):
let resp = http_get_string(url);
let json = parse_json(resp);  // ✅ Automatically unwraps Result!
```

### 3. **Three Approaches to Choose From**

#### **Option 1: fetch_json() - RECOMMENDED**
Simplest and most ergonomic:

```rhai
const API_URL = "https://api.coingecko.com/api/v3/simple/price?ids=sui&vs_currencies=usd";

fn get_price() {
    let data = fetch_json(API_URL);

    // Check for errors
    if data.to_string().starts_with("Error:") {
        throw data.to_string();
    }

    return data["sui"]["usd"];
}

get_price();
```

#### **Option 2: http_get_string() + parse_json()**
More explicit, gives you control over the HTTP request:

```rhai
fn get_price() {
    let response = http_get_string(API_URL);
    let data = parse_json(response);  // Automatically unwraps Result

    if data.to_string().starts_with("Error:") {
        throw data.to_string();
    }

    return data["sui"]["usd"];
}
```

#### **Option 3: Manual Result Handling (ADVANCED)**
For advanced users who want full control:

```rhai
fn get_price() {
    let response = http_get_string(API_URL);

    if is_err(response) {
        throw "HTTP failed: " + err(response);
    }

    // Note: unwrap_string still has limitations, use parse_json(response) instead
    let data = parse_json(response);

    // Rest of the code...
}
```

## Complete Working Example

Here's a complete oracle script that fetches SUI price and returns a bucket value:

```rhai
const API_URL = "https://api.coingecko.com/api/v3/simple/price?ids=sui&vs_currencies=usd";

fn fetch_sui_price(url) {
    // Use fetch_json for simplicity
    let payload = fetch_json(url);
    let payload_str = payload.to_string();

    if payload_str.starts_with("Error:") {
        throw "Failed to fetch data: " + payload_str;
    }

    if !payload.contains_key("sui") {
        throw "Response missing 'sui' key";
    }

    let sui_entry = payload["sui"];

    if !sui_entry.contains_key("usd") {
        throw "Response missing 'usd' key";
    }

    let price = sui_entry["usd"] * 1.0;

    if price <= 0.0 {
        throw "Invalid price";
    }

    return price;
}

fn resolve_price_bucket(url) {
    let price = fetch_sui_price(url);

    if price < 1.5 {
        return 0;
    } else if price <= 2.0 {
        return 1;
    } else {
        return 2;
    }
}

resolve_price_bucket(API_URL);
```

## Why the Old Approach Failed

The original script used:
```rhai
let resp = http_get_string(url);
let payload = parse_json(unwrap_string(resp));  // ❌ BROKEN
```

**Problem**: When Rhai receives a `Result<String, String>` from Rust, it wraps it as an opaque type. Calling `to_string()` on this type returns the Rust type name (`core::result::Result<...>`) rather than the actual value, making `unwrap_string()` unable to extract the content.

**Solution**: The updated `parse_json()` function now has overloaded signatures:
- `parse_json(&str)` - for plain strings
- `parse_json(&mut Dynamic)` - for Result types (automatically unwraps)

## Available Functions Reference

### HTTP Functions
| Function | Description | Returns | Use Case |
|----------|-------------|---------|----------|
| `fetch_json(url)` | Fetch and parse JSON in one step | Dynamic (object/array) or error string | **RECOMMENDED** for most cases |
| `http_get_string(url)` | HTTP GET request | `Result<String, String>` | Advanced usage with manual handling |
| `http_get(url)` | HTTP GET request | String or "Error: ..." | Simple string fetching |
| `http_get_json(url)` | HTTP GET with JSON validation | JSON string or error | When you want the raw JSON string |

### JSON Functions
| Function | Description |
|----------|-------------|
| `parse_json(json_string)` | Parse JSON string to Rhai object |
| `parse_json(result)` | Parse JSON from Result (auto-unwraps) |

### Result Helper Functions
| Function | Description |
|----------|-------------|
| `is_err(result)` | Check if Result is an error |
| `is_ok(result)` | Check if Result is ok |
| `err(result)` | Extract error message |
| `unwrap(result)` | Unwrap Result to Dynamic |

### Utility Functions
| Function | Description |
|----------|-------------|
| `contains_key(map, key)` | Check if map contains key |
| `to_string(value)` | Convert value to string |
| `join(array, sep)` | Join array elements |

## Migration Guide

If you have existing scripts using the broken pattern:

### Before (Broken):
```rhai
let resp = http_get_string(url);
if is_err(resp) {
    throw "HTTP failed: " + err(resp);
}
let payload = parse_json(unwrap_string(resp));
```

### After (Fixed) - Option 1:
```rhai
let payload = fetch_json(url);  // Simplest!
if payload.to_string().starts_with("Error:") {
    throw payload.to_string();
}
```

### After (Fixed) - Option 2:
```rhai
let resp = http_get_string(url);
let payload = parse_json(resp);  // No unwrap_string needed!
if payload.to_string().starts_with("Error:") {
    throw payload.to_string();
}
```

## Testing Your Scripts

Use the `/execute_code` endpoint to test scripts:

```bash
curl -X POST http://localhost:3000/execute_code \
  -H "Content-Type: application/json" \
  -d '{
    "code": "fetch_json(\"https://api.coingecko.com/api/v3/simple/price?ids=sui&vs_currencies=usd\")[\"sui\"][\"usd\"]",
    "return_type": "NUMBER"
  }'
```

## Common Patterns

### Price Bucket Oracle
```rhai
fn get_bucket(price) {
    if price < 1.5 {
        return 0;
    } else if price <= 2.0 {
        return 1;
    } else {
        return 2;
    }
}
```

### Boolean Oracle
```rhai
fn check_threshold(value, threshold) {
    return value > threshold;
}
```

### Error Handling
```rhai
fn safe_fetch(url) {
    let data = fetch_json(url);
    let str = data.to_string();

    if str.starts_with("Error:") {
        throw "Fetch failed: " + str;
    }

    return data;
}
```

## Examples

See the following files for complete examples:
- [`examples/oracle_scripts.rhai`](examples/oracle_scripts.rhai) - Comprehensive examples
- [`examples/working_sui_price_oracle.rhai`](examples/working_sui_price_oracle.rhai) - Fixed SUI price oracle

## Troubleshooting

### Error: "JSON parse failed: Error: expected value at line 1 column 1"
**Cause**: You're passing a Result type representation to JSON parser
**Fix**: Use `fetch_json()` or pass Result directly to `parse_json()` without `unwrap_string()`

### Error: "Response missing 'key' key"
**Cause**: API response structure doesn't match expectations
**Fix**: Add debug logging to inspect the actual response structure

### Error: "HTTP request failed"
**Cause**: Network error or invalid URL
**Fix**: Check the URL is accessible and returns JSON
