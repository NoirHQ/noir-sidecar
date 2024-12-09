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

// pub mod solana;

use crate::client::Client;
use axum::{extract::State, http::StatusCode, Json};
use serde_json::Value;
// use solana::{Solana, SolanaImpl};
use jsonrpsee::{
    types::{ErrorCode, ErrorObject, ErrorObjectOwned},
    RpcModule,
};
use std::{fmt::Display, sync::Arc};

pub fn create_rpc_handler(client: Client) -> RpcModule<Client> {
    let mut module = RpcModule::new(client);

    module
        .register_method("health", |params, _ctx, _| {
            tracing::debug!("{:?}", params);
            "Ok"
        })
        .unwrap();

    module
}

pub async fn handle_rpc_request(
    State(module): State<Arc<RpcModule<Client>>>,
    Json(payload): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let request = serde_json::to_string(&payload).unwrap();

    let (response, _) = match module.raw_json_request(&request, 1).await {
        Ok((response, stream)) => (response, stream),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::to_value(internal_error(e, None)).unwrap()),
            )
        }
    };

    (
        StatusCode::OK,
        serde_json::from_str::<Value>(&response).map(Json).unwrap(),
    )
}

pub fn internal_error(message: impl Display, data: Option<Value>) -> ErrorObjectOwned {
    ErrorObject::owned(
        ErrorCode::InternalError.code(),
        format!("{}: {}", ErrorCode::InternalError.message(), message),
        data,
    )
}
