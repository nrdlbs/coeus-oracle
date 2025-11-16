// Cryptocurrency Price Oracle (Rhai)
// Pulls SUI/USD pricing and maps to outcome buckets.

const API_URL = "https://api.coingecko.com/api/v3/simple/price?ids=sui&vs_currencies=usd";

// ============================================================================
// OPTION 1: Using fetch_json (RECOMMENDED - Simplest approach)
// ============================================================================
fn fetch_sui_price_simple(url) {
    // fetch_json handles both HTTP request and JSON parsing
    let payload = fetch_json(url);
    let payload_str = payload.to_string();

    if payload_str.starts_with("Error:") {
        throw "Failed to fetch data: " + payload_str;
    }

    if !payload.contains_key("sui") {
        throw "CoinGecko response missing 'sui' key";
    }

    let sui_entry = payload["sui"];

    if !sui_entry.contains_key("usd") {
        throw "CoinGecko response missing 'usd' key";
    }

    let price_value = sui_entry["usd"];
    let price = price_value * 1.0;

    if price <= 0.0 {
        throw "CoinGecko returned a non-positive price";
    }

    return price;
}

fn resolve_price_bucket_simple(url) {
    let sui_price = fetch_sui_price_simple(url);

    if sui_price < 1.5 {
        return 0;
    } else if sui_price <= 2.0 {
        return 1;
    } else {
        return 2;
    }
}

// ============================================================================
// OPTION 2: Using http_get_string + parse_json (FIXED)
// ============================================================================
// The key fix: Pass the Result directly to parse_json WITHOUT unwrap_string
// parse_json now automatically handles Result types!
fn fetch_sui_price_fixed(url) {
    let resp = http_get_string(url);

    // Pass Result directly to parse_json - it will unwrap automatically!
    let payload = parse_json(resp);
    let payload_str = payload.to_string();

    if payload_str.starts_with("Error:") {
        throw "JSON parse failed: " + payload_str;
    }

    if !payload.contains_key("sui") {
        throw "CoinGecko response missing 'sui' key";
    }

    let sui_entry = payload["sui"];

    if !sui_entry.contains_key("usd") {
        throw "CoinGecko response missing 'usd' key";
    }

    let price_value = sui_entry["usd"];
    let price = price_value * 1.0;

    if price <= 0.0 {
        throw "CoinGecko returned a non-positive price";
    }

    return price;
}

fn resolve_price_bucket_fixed(url) {
    let sui_price = fetch_sui_price_fixed(url);

    if sui_price < 1.5 {
        return 0;
    } else if sui_price <= 2.0 {
        return 1;
    } else {
        return 2;
    }
}

// Use the recommended approach
resolve_price_bucket_simple(API_URL);
