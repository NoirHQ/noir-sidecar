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

pub mod solana;

use axum::{extract::State, http::StatusCode, Json};
use jsonrpsee::{
    core::{client::ClientT, ClientError},
    rpc_params,
    types::{ErrorCode, ErrorObject, ErrorObjectOwned},
    ws_client::WsClient,
    RpcModule,
};
use noir_core_primitives::Hash;
use parity_scale_codec::{Decode, Encode};
use serde::Serialize;
use serde_json::Value;
use solana::{Solana, SolanaServer};
use std::sync::Arc;

pub fn create_rpc_module(client: Arc<WsClient>) -> Result<RpcModule<()>, anyhow::Error> {
    let mut module = RpcModule::new(());

    module.merge(Solana::new(client.clone()).into_rpc())?;

    Ok(module)
}

pub async fn handle_rpc_request(
    State(module): State<Arc<RpcModule<()>>>,
    Json(payload): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let request = serde_json::to_string(&payload).unwrap();

    let (response, _) = match module.raw_json_request(&request, 1).await {
        Ok((response, stream)) => (response, stream),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::to_value(internal_error(Some(e.to_string()))).unwrap()),
            )
        }
    };

    (
        StatusCode::OK,
        serde_json::from_str::<Value>(&response).map(Json).unwrap(),
    )
}

pub fn error<S: Serialize>(error: ErrorCode, data: Option<S>) -> ErrorObjectOwned {
    ErrorObject::owned(error.code(), error.message(), data)
}

pub fn internal_error<S: Serialize>(data: Option<S>) -> ErrorObjectOwned {
    error(ErrorCode::InternalError, data)
}

pub fn parse_error<S: Serialize>(data: Option<S>) -> ErrorObjectOwned {
    error(ErrorCode::ParseError, data)
}

pub async fn state_call<I: Encode, O: Decode>(
    client: &WsClient,
    method: &str,
    data: I,
    hash: Option<Hash>,
) -> Result<O, ClientError> {
    if !client.is_connected() {
        return Err(ClientError::Custom("Client disconnected".to_string()));
    }

    let args = format!("0x{}", hex::encode(data.encode()));
    let mut res: String = client
        .request("state_call", rpc_params!(method, args, hash))
        .await?;
    if res.starts_with("0x") {
        res = res.strip_prefix("0x").map(|s| s.to_string()).unwrap();
    }
    let res = hex::decode(res).map_err(|e| ClientError::Custom(e.to_string()))?;

    O::decode(&mut &res[..]).map_err(|e| ClientError::Custom(e.to_string()))
}
