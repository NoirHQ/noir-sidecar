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

#[cfg(not(feature = "mock"))]
use crate::client::Client;
#[cfg(not(feature = "mock"))]
use crate::db::index::traits;
use axum::{extract::State, http::StatusCode, Json};
use jsonrpsee::{
    types::{ErrorCode, ErrorObject, ErrorObjectOwned},
    RpcModule,
};
use serde_json::Value;
#[cfg(feature = "mock")]
use solana::mock::{svm::SvmRequest, MockSolana};
#[cfg(not(feature = "mock"))]
use solana::Solana;
use solana::SolanaServer;
use std::sync::Arc;
#[cfg(feature = "mock")]
use tokio::sync::mpsc::UnboundedSender;

pub struct JsonRpcModule {
    inner: RpcModule<()>,
}

impl JsonRpcModule {
    #[cfg(not(feature = "mock"))]
    pub fn create<I>(client: Arc<Client>, accounts_index: Arc<I>) -> Result<Self, anyhow::Error>
    where
        I: 'static + Sync + Send + traits::AccountsIndex,
    {
        let mut module = RpcModule::new(());

        module.merge(Solana::new(client.clone(), accounts_index).into_rpc())?;

        Ok(Self { inner: module })
    }

    #[cfg(feature = "mock")]
    pub fn create(svm: UnboundedSender<SvmRequest>) -> Result<Self, anyhow::Error> {
        let mut module = RpcModule::new(());

        module.merge(MockSolana::new(svm).into_rpc())?;

        Ok(Self { inner: module })
    }
}

pub async fn handle_rpc_request(
    State(module): State<Arc<JsonRpcModule>>,
    Json(payload): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let request = serde_json::to_string(&payload).unwrap();

    let (response, _) = match module.inner.raw_json_request(&request, 1).await {
        Ok((response, stream)) => (response, stream),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::to_value(internal_error(Some(format!("{:?}", e)))).unwrap()),
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

pub fn invalid_params(message: Option<String>) -> ErrorObjectOwned {
    error(ErrorCode::InvalidParams, message)
}

pub fn invalid_request(message: Option<String>) -> ErrorObjectOwned {
    error(ErrorCode::InvalidRequest, message)
}
