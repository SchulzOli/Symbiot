use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // JSON logs so Dozzle can parse and alert on them.
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let addr = std::env::var("SYMBIOT_ADDR").unwrap_or_else(|_| "127.0.0.1:8100".into());
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("cannot bind {addr}: {e}"));
    tracing::info!(%addr, "symbiot-server listening");

    axum::serve(listener, symbiot_server::app())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("server error");
}
