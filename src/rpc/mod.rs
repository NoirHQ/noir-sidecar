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

use crate::client::Client;
use axum::{extract::State, http::StatusCode, Json};
use jsonrpc_core::{Error, ErrorCode, MetaIoHandler};
use serde_json::Value;
use solana::{Solana, SolanaImpl};
use std::{fmt::Display, sync::Arc};

pub fn create_rpc_handler() -> MetaIoHandler<Client> {
    let mut io = MetaIoHandler::<Client>::default();

    io.extend_with(SolanaImpl.to_delegate());

    io
}

pub async fn handle_rpc_request(
    State((handler, client)): State<(Arc<MetaIoHandler<Client>>, Client)>,
    Json(payload): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let request = serde_json::to_string(&payload).unwrap();

    let response = match handler.handle_request(&request, client).await {
        Some(response) => response,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::to_value(Error::internal_error()).unwrap()),
            )
        }
    };

    (
        StatusCode::OK,
        serde_json::from_str::<Value>(&response).map(Json).unwrap(),
    )
}

pub fn internal_error(message: impl Display) -> Error {
    Error {
        code: ErrorCode::InternalError,
        message: format!("{}: {}", ErrorCode::InternalError.description(), message),
        data: None,
    }
}
