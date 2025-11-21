// Copyright (c), Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use axum::{Router, routing::get, routing::post};
use bech32::{Hrp, decode};
use fastcrypto::ed25519::Ed25519PrivateKey;
use fastcrypto::traits::ToFromBytes;
use fastcrypto::{ed25519::Ed25519KeyPair, traits::KeyPair};
use nautilus_server::AppState;
use nautilus_server::app::{execute_code, process_data};
use nautilus_server::common::{get_attestation, health_check};
use std::sync::Arc;
use sui_rpc::client::Client;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    let eph_kp = Ed25519KeyPair::generate(&mut rand::thread_rng());

    // Use archive node for better support of historical data queries
    // If you need real-time data, you can switch back to TESTNET_FULLNODE
    let sui_client = Client::new(Client::TESTNET_FULLNODE).unwrap();

    let state = Arc::new(AppState { eph_kp, sui_client });

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
        .route("/execute_code", post(execute_code))
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
