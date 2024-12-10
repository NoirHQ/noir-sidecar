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

use jsonrpc_core::Error;
use jsonrpc_derive::rpc;
use parity_scale_codec::{Decode, Encode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Decode, Encode, Serialize, Deserialize)]
pub struct ChainInfo {
    pub chain_id: String,
    pub bech32_prefix: String,
    pub name: String,
    pub version: String,
}

#[rpc]
pub trait Cosmos {
    #[rpc(name = "cosmos_chainInfo")]
    fn chain_info(&self) -> Result<ChainInfo, Error>;
}

pub struct CosmosImpl;

impl Cosmos for CosmosImpl {
    fn chain_info(&self) -> Result<ChainInfo, Error> {
        Ok(ChainInfo {
            chain_id: "test".to_string(),
            bech32_prefix: "test".to_string(),
            name: "test".to_string(),
            version: "test".to_string(),
        })
    }
}
