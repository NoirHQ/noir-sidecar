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
    http_client::{HttpClient, HttpClientBuilder},
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize)]
pub struct ClientConfig {
    endpoint: String,
}

#[derive(Debug, Clone)]
pub struct Client {
    pub client: Arc<HttpClient>,
}

impl Metadata for Client {}

pub async fn create_ws_client(config: &ClientConfig) -> Result<Client, ClientError> {
    let client = HttpClientBuilder::default()
        .build(config.endpoint.clone())
        .unwrap();

    Ok(Client {
        client: Arc::new(client),
    })
}