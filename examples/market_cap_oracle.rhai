// Market Capitalization Oracle (Rhai)
// Queries Alpha Vantage for market-cap data and returns the leader.

const API_URL = "https://www.alphavantage.co/query";
const API_KEY = "YOUR_API_KEY"; // Replace with live key.

fn fetch_market_cap(symbol) {
    let url = API_URL
        + "?function=OVERVIEW&symbol="
        + symbol
        + "&apikey="
        + API_KEY;
    
    let data = fetch_json(url);
    let data_str = data.to_string();
    
    if data_str.starts_with("Error:") {
        throw "Failed to fetch data: " + data_str;
    }
    
    if !data.contains_key("MarketCapitalization") {
        return 0.0;
    } else {
        return data["MarketCapitalization"].to_float();
    }
}

fn resolve_market_cap_leader() {
    // Compute market caps first (Rhai doesn't allow function calls in map literals)
    let nvidia_cap = fetch_market_cap("NVDA");
    let apple_cap = fetch_market_cap("AAPL");
    let microsoft_cap = fetch_market_cap("MSFT");
    
    // Now create the map with pre-computed values
    let caps = #{
        "Nvidia": nvidia_cap,
        "Apple": apple_cap,
        "Microsoft": microsoft_cap
    };
    
    let mut leader = "Other";
    let mut max_cap = 0.0;
    
    for name in caps.keys() {
        let cap = caps[name];
        if cap > max_cap {
            max_cap = cap;
            leader = name;
        }
    }
    
    if leader == "Nvidia" {
        return 0;
    } else if leader == "Apple" {
        return 1;
    } else if leader == "Microsoft" {
        return 2;
    } else {
        return 3;
    }
}

resolve_market_cap_leader();

