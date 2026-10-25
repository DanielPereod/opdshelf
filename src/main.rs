use opdshelf::{router, Config};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,tower_http=warn")))
        .init();

    let config = Config::from_env();
    let addr = format!("{}:{}", config.host, config.port);
    tracing::info!("books directory: {}", config.books_dir.display());
    if !config.auth_enabled() {
        tracing::warn!("ADMIN_USERNAME and ADMIN_PASSWORD are not set: the library and admin panel are open to anyone");
    }

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => listener,
        Err(err) => {
            tracing::error!("cannot listen on {addr}: {err}");
            std::process::exit(1);
        }
    };
    tracing::info!("OPDShelf listening on http://{addr}");

    let app = router(config).layer(TraceLayer::new_for_http());
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await
        .expect("server error");
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}
