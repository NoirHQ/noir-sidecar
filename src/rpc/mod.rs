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

use crate::{client::Client, db::index::traits};
use axum::{extract::State, http::StatusCode, Json};
use jsonrpsee::{
    core::{params::ArrayParams, ClientError},
    types::{ErrorCode, ErrorObject, ErrorObjectOwned},
    RpcModule,
};
use noir_core_primitives::Hash;
use parity_scale_codec::{Decode, Encode};
use serde_json::Value;
#[cfg(feature = "mock")]
use solana::mock::MockSolana;
#[cfg(not(feature = "mock"))]
use solana::Solana;
use solana::SolanaServer;
use std::sync::Arc;

pub fn create_rpc_module<I>(
    client: Arc<Client>,
    accounts_index: Arc<I>,
) -> Result<RpcModule<()>, anyhow::Error>
where
    I: 'static + Sync + Send + traits::AccountsIndex,
{
    let mut module = RpcModule::new(());

    #[cfg(feature = "mock")]
    module.merge(MockSolana::default().into_rpc())?;
    #[cfg(not(feature = "mock"))]
    module.merge(Solana::new(client.clone(), accounts_index).into_rpc())?;

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

pub fn error(error: ErrorCode, message: Option<String>) -> ErrorObjectOwned {
    ErrorObject::owned(
        error.code(),
        message.unwrap_or(error.message().to_string()),
        None::<()>,
    )
}

pub fn internal_error(message: Option<String>) -> ErrorObjectOwned {
    error(ErrorCode::InternalError, message)
}

pub fn parse_error(message: Option<String>) -> ErrorObjectOwned {
    error(ErrorCode::ParseError, message)
}

pub fn invalid_params(message: Option<String>) -> ErrorObjectOwned {
    error(ErrorCode::InvalidParams, message)
}

pub fn invalid_request(message: Option<String>) -> ErrorObjectOwned {
    error(ErrorCode::InvalidRequest, message)
}

pub async fn state_call<I: Encode, O: Decode>(
    client: &Client,
    method: &str,
    data: I,
    hash: Option<Hash>,
) -> Result<O, ClientError> {
    let args = format!("0x{}", hex::encode(data.encode()));

    let mut params = ArrayParams::new();
    params.insert(method).unwrap();
    params.insert(args).unwrap();
    params.insert(hash).unwrap();

    let mut res: String = client.request::<String>("state_call", params).await?;
    if res.starts_with("0x") {
        res = res.strip_prefix("0x").map(|s| s.to_string()).unwrap();
    }
    let res = hex::decode(res).map_err(|e| ClientError::Custom(e.to_string()))?;

    O::decode(&mut &res[..]).map_err(|e| ClientError::Custom(e.to_string()))
}
