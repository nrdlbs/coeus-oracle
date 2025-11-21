module oracle::config;

use enclave::enclave::{Cap, Self};

#[error]
const ENotAdmin: vector<u8> = b"Not admin";

public struct CONFIG has drop {}

public struct Config has key, store {
    id: UID,
    max_update_time_ms: u64,
    admin: address,
}

fun init(otw: CONFIG, ctx: &mut TxContext) {
    let config = Config {
        id: object::new(ctx),
        max_update_time_ms: 5000,
        admin: ctx.sender(),
    };
    let cap = enclave::new_cap(otw, ctx);
    transfer::public_transfer(cap, ctx.sender());
    transfer::share_object(config);
}

public fun update_max_update_time_ms(
    config: &mut Config,
    max_update_time_ms: u64,
    ctx: &mut TxContext,
) {
    assert!(config.admin == ctx.sender(), ENotAdmin);
    config.max_update_time_ms = max_update_time_ms;
}

public fun get_max_update_time_ms(config: &Config): u64 {
    config.max_update_time_ms
}
