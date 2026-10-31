use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::Duration;

use tracing_subscriber::EnvFilter;

fn env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.into())
}

/// `symbiot-server --health`: used by Docker's HEALTHCHECK (the runtime image has no curl).
fn healthcheck(addr: &str) -> bool {
    let addr = addr.replace("0.0.0.0", "127.0.0.1");
    let Ok(mut s) = TcpStream::connect(&addr) else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(3)));
    if s.write_all(b"GET /health HTTP/1.0\r\nHost: localhost\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut buf = String::new();
    let _ = s.read_to_string(&mut buf);
    buf.starts_with("HTTP/1.0 200") || buf.starts_with("HTTP/1.1 200")
}

#[tokio::main]
async fn main() {
    let addr = env("SYMBIOT_ADDR", "127.0.0.1:8100");
    if std::env::args().any(|a| a == "--health") {
        std::process::exit(if healthcheck(&addr) { 0 } else { 1 });
    }

    // JSON logs so Dozzle can parse and alert on them.
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let data = PathBuf::from(env("SYMBIOT_DATA", "data"));
    let ui = std::env::var("SYMBIOT_UI").ok().map(PathBuf::from);
    let store = symbiot_store::Store::open_dir(&data)
        .unwrap_or_else(|e| panic!("cannot open store in {}: {e}", data.display()));
    tracing::info!(device = %store.device(), data = %data.display(), ui = ?ui, "store opened");
    if let Some(old) = store.set_aside_v1() {
        tracing::warn!(file = %old.display(), "old v1 database set aside, starting a fresh v2 chain");
    }
    // Without SYMBIOT_AI_URL everything works except „Nächsten Schritt vorschlagen“.
    let ai = symbiot_ai::Config::from_env().map(|c| {
        tracing::info!(url = %c.url, model = %c.local_model, remote = ?c.remote_model, "ai configured");
        std::sync::Arc::new(symbiot_ai::LiteLlm::new(c)) as std::sync::Arc<dyn symbiot_ai::Advisor>
    });

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("cannot bind {addr}: {e}"));
    tracing::info!(%addr, "symbiot-server listening");

    axum::serve(listener, symbiot_server::app(store, ui, ai))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("server error");
}
