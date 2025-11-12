// Copyright (c), Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use axum::{routing::get, routing::post, Router};
use bech32::{decode, Hrp};
use fastcrypto::ed25519::Ed25519PrivateKey;
use fastcrypto::traits::ToFromBytes;
use fastcrypto::{ed25519::Ed25519KeyPair, traits::KeyPair};
use nautilus_server::app::process_data;
use nautilus_server::common::{get_attestation, health_check};
use nautilus_server::AppState;
use std::sync::Arc;
use sui_rpc::client::v2::Client;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

fn five_to_eight_relaxed(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 5 / 8 + 1);
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;

    for &v in data {
        acc = (acc << 5) | (v as u32);
        bits += 5;
        while bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xFF) as u8);
        }
    }
    out // ignore leftover <8 bits
}

pub fn parse_sui_privkey(bech: &str) -> Result<Ed25519PrivateKey> {
    let (hrp, payload) = decode(bech).context("bech32 decode failed")?;

    // HRP must be "suiprivkey"
    let expected = Hrp::parse_unchecked("suiprivkey");
    anyhow::ensure!(hrp == expected, "unexpected HRP: {}", hrp);

    // ADAPTIVE: 5-bit or already 8-bit?
    let maxv = payload.iter().copied().max().unwrap_or(0);
    let bytes = if maxv <= 31 {
        five_to_eight_relaxed(&payload)
    } else {
        payload // already 8-bit
    };

    anyhow::ensure!(bytes.len() == 33, "expected 33 bytes, got {}", bytes.len());
    anyhow::ensure!(
        bytes[0] == 0x00,
        "not an ed25519 key (scheme={:#04x})",
        bytes[0]
    );

    let sk: [u8; 32] = bytes[1..].try_into().unwrap();
    let key = Ed25519PrivateKey::from_bytes(&sk).unwrap();
    Ok(key)
}

pub fn construct_kp_from_bech32_string(bech: &str) -> Result<Ed25519PrivateKey> {
    let key = parse_sui_privkey(bech)?;
    Ok(key)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file
    dotenv::dotenv().ok();

    // let eph_kp = Ed25519KeyPair::generate(&mut rand::thread_rng());
    // static eph_kp
    let seed_hex = std::env::var("EPH_ED25519_SEED")
        .context("EPH_ED25519_SEED not set (expect 32-byte hex seed)")?;
    let seed_hex = seed_hex.trim_start_matches("0x");
    let bytes = hex::decode(seed_hex).context("seed not valid hex")?;
    anyhow::ensure!(bytes.len() == 32, "seed must be exactly 32 bytes");

    // fastcrypto's Ed25519KeyPair can be deterministically derived from a 32-byte seed:
    let seed: [u8; 32] = bytes.try_into().expect("length checked above");
    let eph_kp = Ed25519KeyPair::from_bytes(&seed).unwrap();

    let pk_string = std::env::var("SUI_PK")
        .map_err(|_| anyhow::anyhow!("SUI_PK environment variable not set"))?;

    let kp = construct_kp_from_bech32_string(&pk_string)
        .map_err(|e| anyhow::anyhow!("Failed to construct keypair: {}", e))?;

    // Use archive node for better support of historical data queries
    // If you need real-time data, you can switch back to TESTNET_FULLNODE
    let sui_client = Client::new(Client::TESTNET_FULLNODE).unwrap();

    let state = Arc::new(AppState {
        eph_kp,
        sui_client,
    });

    // Spawn host-only init server if seal-example feature is enabled
    #[cfg(feature = "seal-example")]
    {
        nautilus_server::app::spawn_host_init_server(state.clone()).await?;
    }

    // Define your own restricted CORS policy here if needed.
    let cors = CorsLayer::new().allow_methods(Any).allow_headers(Any);

    let app = Router::new()
        .route("/", get(ping))
        .route("/get_attestation", get(get_attestation))
        .route("/process_data", post(process_data))
        .route("/health_check", get(health_check))
        .with_state(state)
        .layer(cors);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app.into_make_service())
        .await
        .map_err(|e| anyhow::anyhow!("Server error: {}", e))
}

async fn ping() -> &'static str {
    "Pong!"
}
