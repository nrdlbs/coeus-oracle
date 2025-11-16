// ============================================================================
// Rhai Oracle Script Examples
// ============================================================================
// This file contains examples of how to write oracle scripts using Rhai.
// These scripts fetch data from APIs and return values that can be used
// on-chain.

// ============================================================================
// EXAMPLE 1: Simple Price Oracle (RECOMMENDED APPROACH)
// ============================================================================
// This is the simplest and most ergonomic way to fetch JSON data.
// Use `fetch_json(url)` which handles HTTP requests and JSON parsing in one step.

const API_URL = "https://api.coingecko.com/api/v3/simple/price?ids=sui&vs_currencies=usd";

fn get_sui_price_simple() {
    // fetch_json() automatically:
    // 1. Fetches the URL
    // 2. Parses the JSON response
    // 3. Returns a Rhai map/object or error string
    let data = fetch_json(API_URL);

    // Check if error occurred
    let data_str = data.to_string();
    if data_str.starts_with("Error:") {
        throw data_str;
    }

    // Access nested fields
    if !data.contains_key("sui") {
        throw "Response missing 'sui' key";
    }

    let sui_entry = data["sui"];

    if !sui_entry.contains_key("usd") {
        throw "Response missing 'usd' key";
    }

    return sui_entry["usd"];
}

// Map price to outcome buckets
fn resolve_price_bucket_simple() {
    let price = get_sui_price_simple();

    if price < 1.5 {
        return 0;
    } else if price <= 2.0 {
        return 1;
    } else {
        return 2;
    }
}

// Call the main function
resolve_price_bucket_simple();


// ============================================================================
// EXAMPLE 2: Using http_get_string with parse_json (ADVANCED)
// ============================================================================
// This approach gives you more control but requires manual error handling.
// The parse_json() function now automatically unwraps Result types.

fn get_sui_price_advanced() {
    // http_get_string returns Result<String, String>
    let response = http_get_string(API_URL);

    // parse_json now handles Result types automatically!
    // It will unwrap the Result and parse the JSON
    let data = parse_json(response);

    // Check if parsing failed
    let data_str = data.to_string();
    if data_str.starts_with("Error:") {
        throw data_str;
    }

    // Extract the price
    if !data.contains_key("sui") {
        throw "Response missing 'sui' key";
    }

    let sui_entry = data["sui"];

    if !sui_entry.contains_key("usd") {
        throw "Response missing 'usd' key";
    }

    return sui_entry["usd"];
}


// ============================================================================
// EXAMPLE 3: Manual Error Handling with Result Types (OLD APPROACH - NOT RECOMMENDED)
// ============================================================================
// This approach manually checks Result types. It's more verbose but shows
// how the underlying system works.

fn get_sui_price_manual() {
    let response = http_get_string(API_URL);

    // Manually check if the Result is an error
    if is_err(response) {
        throw "HTTP request failed: " + err(response);
    }

    // Manually unwrap the Result to get the string
    let json_str = unwrap_string(response);

    // Check if unwrapping failed
    if json_str.starts_with("Error:") {
        throw json_str;
    }

    // Parse JSON from the string
    let data = parse_json(json_str);
    let data_str = data.to_string();

    if data_str.starts_with("Error:") {
        throw "JSON parse failed: " + data_str;
    }

    // Extract the price (same as above)
    if !data.contains_key("sui") {
        throw "Response missing 'sui' key";
    }

    let sui_entry = data["sui"];

    if !sui_entry.contains_key("usd") {
        throw "Response missing 'usd' key";
    }

    return sui_entry["usd"];
}


// ============================================================================
// EXAMPLE 4: Weather Oracle
// ============================================================================
// Fetch weather data and return temperature bucket

fn get_temperature_bucket() {
    // Using Open-Meteo API (free, no API key required)
    const WEATHER_URL = "https://api.open-meteo.com/v1/forecast?latitude=37.7749&longitude=-122.4194&current_weather=true";

    let data = fetch_json(WEATHER_URL);

    let data_str = data.to_string();
    if data_str.starts_with("Error:") {
        throw data_str;
    }

    if !data.contains_key("current_weather") {
        throw "Response missing 'current_weather'";
    }

    let weather = data["current_weather"];

    if !weather.contains_key("temperature") {
        throw "Response missing 'temperature'";
    }

    let temp = weather["temperature"];

    // Return temperature bucket (Celsius)
    if temp < 10.0 {
        return 0; // Cold
    } else if temp <= 25.0 {
        return 1; // Moderate
    } else {
        return 2; // Hot
    }
}


// ============================================================================
// EXAMPLE 5: Boolean Oracle (Binary Outcome)
// ============================================================================
// Check if a cryptocurrency price is above a threshold

fn is_price_above_threshold() {
    let data = fetch_json(API_URL);

    let data_str = data.to_string();
    if data_str.starts_with("Error:") {
        throw data_str;
    }

    let price = data["sui"]["usd"];

    // Return boolean: true if price > $2.00, false otherwise
    return price > 2.0;
}


// ============================================================================
// AVAILABLE FUNCTIONS
// ============================================================================
//
// HTTP Functions:
// - fetch_json(url)              - Fetch URL and parse as JSON (RECOMMENDED)
// - http_get_string(url)         - Fetch URL, returns Result<String, String>
// - http_get(url)                - Fetch URL, returns String or "Error: ..."
// - http_get_json(url)           - Fetch and validate JSON string
//
// JSON Functions:
// - parse_json(json_string)      - Parse JSON string to Rhai object
// - parse_json(result)           - Parse JSON from Result (auto-unwraps)
//
// Result Helper Functions (for advanced usage):
// - is_err(result)               - Check if Result is an error
// - is_ok(result)                - Check if Result is ok
// - err(result)                  - Extract error message from Result
// - unwrap(result)               - Unwrap Result to Dynamic
// - unwrap_string(result)        - Unwrap Result to String
//
// Map/Object Functions:
// - contains_key(map, key)       - Check if map contains key
// - map[key]                     - Access map value by key
//
// String Functions:
// - to_string(value)             - Convert value to string
// - starts_with(string, prefix)  - Check if string starts with prefix
// - join(array, separator)       - Join array elements with separator
//
// ============================================================================
