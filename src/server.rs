// This file is part of Noir.

// Copyright (c) Haderech Pte. Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{router, rpc::JsonRpcModule};
use axum::{
    error_handling::HandleErrorLayer,
    http::{HeaderValue, StatusCode},
};
use serde::Deserialize;
use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};
use tokio::{net::TcpListener, signal};
use tower::{BoxError, ServiceBuilder};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    ServiceBuilderExt,
};

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub listen_address: String,
    pub port: u16,
    pub cors: Option<ItemOrList<String>>,
    pub request_timeout_seconds: u64,
}

pub struct SidecarServer {
    pub config: ServerConfig,
}

impl SidecarServer {
    pub fn new(config: ServerConfig) -> Self {
        Self { config }
    }

    pub async fn run(&self, module: Arc<JsonRpcModule>) -> anyhow::Result<()> {
        let ip_addr = std::net::IpAddr::from_str(&self.config.listen_address)?;
        let addr = SocketAddr::new(ip_addr, self.config.port);
        let listener = TcpListener::bind(addr).await?;

        let middleware = ServiceBuilder::new()
            .layer(HandleErrorLayer::new(|err: BoxError| async move {
                if err.is::<tower::timeout::error::Elapsed>() {
                    StatusCode::REQUEST_TIMEOUT
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                }
            }))
            .timeout(Duration::from_secs(self.config.request_timeout_seconds))
            .trace_for_http()
            .layer(cors_layer(self.config.cors.clone())?);

        let router = router::create_router(module).layer(middleware.into_inner());

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

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum ItemOrList<T> {
    Item(T),
    List(Vec<T>),
}

impl<T> ItemOrList<T> {
    fn into_list(self) -> Vec<T> {
        match self {
            ItemOrList::Item(item) => vec![item],
            ItemOrList::List(list) => list,
        }
    }
}

fn cors_layer(cors: Option<ItemOrList<String>>) -> anyhow::Result<CorsLayer> {
    let origins = cors.map(|c| c.into_list()).unwrap_or_default();

    match origins.as_slice() {
        [] => Ok(CorsLayer::new()),
        [origin] if origin == "*" || origin == "all" => Ok(CorsLayer::permissive()),
        origins => {
            let list = origins
                .iter()
                .map(|o| HeaderValue::from_str(o))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(CorsLayer::new().allow_origin(AllowOrigin::list(list)))
        }
    }
}
