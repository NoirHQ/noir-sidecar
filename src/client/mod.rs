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

use jsonrpc_core::Metadata;
use jsonrpsee::{
    core::ClientError,
    ws_client::{WsClient, WsClientBuilder},
};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};

#[derive(Debug, Clone, Deserialize)]
pub struct ClientConfig {
    endpoint: String,
    request_timeout_seconds: Option<u64>,
    connection_timeout_seconds: Option<u64>,
    max_concurrent_requests: Option<usize>,
    max_response_size: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct Client {
    pub client: Arc<WsClient>,
}

impl Metadata for Client {}

pub async fn create_client(config: &ClientConfig) -> Result<Client, ClientError> {
    let client = WsClientBuilder::default()
        .request_timeout(
            config
                .request_timeout_seconds
                .map(Duration::from_secs)
                .unwrap_or(Duration::from_secs(30)),
        )
        .connection_timeout(
            config
                .connection_timeout_seconds
                .map(Duration::from_secs)
                .unwrap_or(Duration::from_secs(30)),
        )
        .max_concurrent_requests(config.max_concurrent_requests.unwrap_or(2048))
        .max_response_size(config.max_response_size.unwrap_or(20 * 1024 * 1024))
        .build(config.endpoint.clone())
        .await
        .unwrap();

    Ok(Client {
        client: Arc::new(client),
    })
}
