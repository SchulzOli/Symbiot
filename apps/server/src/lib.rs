//! Symbiot server — runs on the home server `heimserver`, reachable only via Tailscale.
//! A replica and helper for the devices, never the owner of the data.

use axum::{Json, Router, routing::get};
use serde::Serialize;

#[derive(Serialize)]
pub struct Health {
    pub ok: bool,
    pub app: &'static str,
    pub version: &'static str,
    pub core: &'static str,
    pub protocol: u16,
}

pub fn app() -> Router {
    Router::new().route("/health", get(health))
}

async fn health() -> Json<Health> {
    Json(Health {
        ok: true,
        app: "symbiot",
        version: env!("CARGO_PKG_VERSION"),
        core: symbiot_core::VERSION,
        protocol: symbiot_core::PROTOCOL_VERSION,
    })
}
