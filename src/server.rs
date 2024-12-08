use axum::Router;
use serde::Deserialize;
use std::{net::SocketAddr, str::FromStr};
use tokio::{net::TcpListener, signal};

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub listen_address: String,
    pub port: u16,
}

pub struct SidecarServer {
    pub config: ServerConfig,
}

impl SidecarServer {
    pub fn new(config: ServerConfig) -> Self {
        Self { config }
    }

    pub async fn run(&self, router: Router) -> anyhow::Result<()> {
        let ip_addr = std::net::IpAddr::from_str(&self.config.listen_address)?;
        let addr = SocketAddr::new(ip_addr, self.config.port);
        let listener = TcpListener::bind(addr).await?;

        tracing::info!("Starting sidecar server on {}", addr);

        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_signal())
            .await?;

        Ok(())
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
