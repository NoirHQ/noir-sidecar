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

use jsonrpc_core::{BoxFuture, Error};
use jsonrpc_derive::rpc;
use jsonrpsee::{core::client::ClientT, rpc_params};
use parity_scale_codec::{Decode, Encode};
use serde::{Deserialize, Serialize};

use crate::client::Client;

#[derive(Debug, Clone, Decode, Encode, Serialize, Deserialize)]
pub struct ChainInfo {
    pub chain_id: String,
    pub bech32_prefix: String,
    pub name: String,
    pub version: String,
}

#[rpc]
pub trait Cosmos {
    type Metadata;

    #[rpc(meta, name = "cosmos_chainInfo")]
    fn chain_info(&self, meta: Self::Metadata) -> BoxFuture<Result<ChainInfo, Error>>;
}

pub struct CosmosImpl;

impl Cosmos for CosmosImpl {
    type Metadata = Client;

    fn chain_info(&self, meta: Self::Metadata) -> BoxFuture<Result<ChainInfo, Error>> {
        let params = rpc_params!([""]);
        let client = meta.client.clone();

        Box::pin(async move {
            client
                .request("cosmos_chainInfo", params)
                .await
                .map_err(|_| Error::internal_error())
        })
    }
}
