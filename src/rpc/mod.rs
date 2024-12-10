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

pub mod cosmos;

use axum::{extract::State, http::StatusCode, Json};
use cosmos::{Cosmos, CosmosImpl};
use jsonrpc_core::{Error, IoHandler};
use serde_json::Value;
use std::sync::Arc;

pub fn create_rpc_handler() -> IoHandler {
    let mut io = IoHandler::<()>::default();

    io.extend_with(CosmosImpl.to_delegate());

    io
}

pub async fn handle_rpc_request(
    State(handler): State<Arc<IoHandler>>,
    Json(payload): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let request = match serde_json::to_string(&payload) {
        Ok(request) => request,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::to_value(Error::invalid_request()).unwrap()),
            )
        }
    };

    let response = match handler.handle_request(&request).await {
        Some(response) => response,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::to_value(Error::internal_error()).unwrap()),
            )
        }
    };
    let response = match serde_json::from_str::<Value>(&response) {
        Ok(response) => Json(response),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::to_value(Error::internal_error()).unwrap()),
            )
        }
    };

    (StatusCode::OK, response)
}
