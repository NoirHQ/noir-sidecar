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

use crate::rpc::{internal_error, parse_error, state_call};
use jsonrpsee::{
    core::{async_trait, client::ClientT, params::ArrayParams, RpcResult},
    proc_macros::rpc,
    ws_client::WsClient,
};
use noir_core_primitives::{Hash, Header};
use serde::{Deserialize, Serialize};
use solana_account_decoder::UiAccount;
use solana_rpc_client_api::{
    config::{
        RpcAccountInfoConfig, RpcContextConfig, RpcEpochConfig, RpcProgramAccountsConfig,
        RpcSendTransactionConfig, RpcSimulateTransactionConfig, RpcTokenAccountsFilter,
    },
    response::{
        OptionalContext, Response as RpcResponse, RpcBlockhash, RpcInflationReward,
        RpcKeyedAccount, RpcResponseContext, RpcSimulateTransactionResult,
    },
};
use solana_runtime_api::error::Error;
use std::sync::Arc;

pub type Slot = u64;

pub type Epoch = u64;

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EpochInfo {
    /// The current epoch
    pub epoch: Epoch,

    /// The current slot, relative to the start of the current epoch
    pub slot_index: u64,

    /// The number of slots in this epoch
    pub slots_in_epoch: u64,

    /// The absolute current slot
    pub absolute_slot: Slot,

    /// The current block height
    pub block_height: u64,

    /// Total number of transactions processed without error since genesis
    pub transaction_count: Option<u64>,
}

#[rpc(client, server)]
#[async_trait]
pub trait Solana {
    #[method(name = "getAccountInfo")]
    async fn get_account_info(
        &self,
        pubkey_str: String,
        config: Option<RpcAccountInfoConfig>,
    ) -> RpcResult<RpcResponse<Option<UiAccount>>>;

    #[method(name = "getMultipleAccounts")]
    async fn get_multiple_accounts(
        &self,
        pubkey_strs: Vec<String>,
        config: Option<RpcAccountInfoConfig>,
    ) -> RpcResult<RpcResponse<Vec<Option<UiAccount>>>>;

    #[method(name = "getProgramAccounts")]
    async fn get_program_accounts(
        &self,
        program_id_str: String,
        config: Option<RpcProgramAccountsConfig>,
    ) -> RpcResult<OptionalContext<Vec<RpcKeyedAccount>>>;

    #[method(name = "getTokenAccountsByOwner")]
    async fn get_token_accounts_by_owner(
        &self,
        owner_str: String,
        token_account_filter: RpcTokenAccountsFilter,
        config: Option<RpcAccountInfoConfig>,
    ) -> RpcResult<RpcResponse<Vec<RpcKeyedAccount>>>;

    #[method(name = "getLatestBlockhash")]
    async fn get_latest_blockhash(
        &self,
        config: Option<RpcContextConfig>,
    ) -> RpcResult<RpcResponse<RpcBlockhash>>;

    #[method(name = "sendTransaction")]
    async fn send_transaction(
        &self,
        data: String,
        config: Option<RpcSendTransactionConfig>,
    ) -> RpcResult<String>;

    #[method(name = "simulateTransaction")]
    async fn simulate_transaction(
        &self,
        data: String,
        config: Option<RpcSimulateTransactionConfig>,
    ) -> RpcResult<RpcResponse<RpcSimulateTransactionResult>>;

    #[method(name = "getInflationReward")]
    async fn get_inflation_reward(
        &self,
        address_strs: Vec<String>,
        config: Option<RpcEpochConfig>,
    ) -> RpcResult<Vec<Option<RpcInflationReward>>>;

    #[method(name = "getFeeForMessage")]
    async fn get_fee_for_message(
        &self,
        data: String,
        config: Option<RpcContextConfig>,
    ) -> RpcResult<RpcResponse<Option<u64>>>;

    #[method(name = "getBalance")]
    async fn get_balance(
        &self,
        pubkey_str: String,
        config: Option<RpcContextConfig>,
    ) -> RpcResult<RpcResponse<u64>>;

    #[method(name = "getGenesisHash")]
    async fn get_genesis_hash(&self) -> RpcResult<String>;

    #[method(name = "getEpochInfo")]
    async fn get_epoch_info(&self, config: Option<RpcContextConfig>) -> RpcResult<EpochInfo>;

    #[method(name = "getTransactionCount")]
    async fn get_transaction_count(&self, config: Option<RpcContextConfig>) -> RpcResult<u64>;
}

#[derive(Clone)]
pub struct Solana {
    client: Arc<WsClient>,
}

impl Solana {
    pub fn new(client: Arc<WsClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl SolanaServer for Solana {
    async fn get_account_info(
        &self,
        pubkey_str: String,
        config: Option<RpcAccountInfoConfig>,
    ) -> RpcResult<RpcResponse<Option<UiAccount>>> {
        tracing::debug!("getAccountInfo: {:?}, config: {:?}", pubkey_str, config);

        let method = "getAccountInfo".to_string();
        let params = serde_json::to_vec(&(pubkey_str, config))
            .map_err(|e| parse_error(Some(e.to_string())))?;

        let response = state_call::<_, Result<Vec<u8>, Error>>(
            &self.client,
            "SolanaRuntimeApi_call",
            (method, params),
        )
        .await
        .map_err(|e| internal_error(Some(e.to_string())))?
        .map_err(|e| internal_error(Some(format!("{:?}", e))))?;

        serde_json::from_slice::<_>(&response).map_err(|e| internal_error(Some(e.to_string())))
    }

    async fn get_multiple_accounts(
        &self,
        pubkey_strs: Vec<String>,
        config: Option<RpcAccountInfoConfig>,
    ) -> RpcResult<RpcResponse<Vec<Option<UiAccount>>>> {
        tracing::debug!("getMultipleAccounts: {:?}, {:?}", pubkey_strs, config);

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: 0,
                api_version: None,
            },
            value: Vec::default(),
        })
    }

    async fn get_program_accounts(
        &self,
        program_id_str: String,
        config: Option<RpcProgramAccountsConfig>,
    ) -> RpcResult<OptionalContext<Vec<RpcKeyedAccount>>> {
        tracing::debug!("getProgramAccounts: {:?}. {:?}", program_id_str, config);

        Ok(OptionalContext::NoContext(Vec::default()))
    }

    async fn get_token_accounts_by_owner(
        &self,
        owner_str: String,
        token_account_filter: RpcTokenAccountsFilter,
        config: Option<RpcAccountInfoConfig>,
    ) -> RpcResult<RpcResponse<Vec<RpcKeyedAccount>>> {
        tracing::debug!(
            "getTokenAccountsByOwner: {:?}, {:?}, {:?}",
            owner_str,
            token_account_filter,
            config
        );

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: 0,
                api_version: None,
            },
            value: Vec::default(),
        })
    }

    async fn get_latest_blockhash(
        &self,
        config: Option<RpcContextConfig>,
    ) -> RpcResult<RpcResponse<RpcBlockhash>> {
        tracing::debug!("getLatestBlockhash: {:?}", config);

        if !self.client.is_connected() {
            return Err(internal_error(Some("Client disconnected".to_string())));
        }

        let hash: Hash = self
            .client
            .request("chain_getFinalizedHead", ArrayParams::new())
            .await
            .map_err(|e| internal_error(Some(e.to_string())))?;

        let mut params = ArrayParams::new();
        params.insert(hash).unwrap();

        let Header { number, .. } = self
            .client
            .request("chain_getHeader", params)
            .await
            .map_err(|e| internal_error(Some(e.to_string())))?;

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: 0,
                api_version: None,
            },
            value: RpcBlockhash {
                blockhash: bs58::encode(hash.as_bytes()).into_string(),
                last_valid_block_height: number as u64,
            },
        })
    }

    async fn send_transaction(
        &self,
        data: String,
        config: Option<RpcSendTransactionConfig>,
    ) -> RpcResult<String> {
        tracing::debug!("sendTransaction: {:?}, {:?}", data, config);

        Ok(String::default())
    }

    async fn simulate_transaction(
        &self,
        data: String,
        config: Option<RpcSimulateTransactionConfig>,
    ) -> RpcResult<RpcResponse<RpcSimulateTransactionResult>> {
        tracing::debug!("simulateTransaction: {:?}, {:?}", data, config);

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: 0,
                api_version: None,
            },
            value: RpcSimulateTransactionResult {
                err: None,
                logs: None,
                accounts: None,
                units_consumed: None,
                return_data: None,
                inner_instructions: None,
                replacement_blockhash: None,
            },
        })
    }

    async fn get_inflation_reward(
        &self,
        address_strs: Vec<String>,
        config: Option<RpcEpochConfig>,
    ) -> RpcResult<Vec<Option<RpcInflationReward>>> {
        tracing::debug!("getInflationReward: {:?}, {:?}", address_strs, config);

        Ok(Vec::default())
    }

    async fn get_fee_for_message(
        &self,
        data: String,
        config: Option<RpcContextConfig>,
    ) -> RpcResult<RpcResponse<Option<u64>>> {
        tracing::debug!("getFeeForMessage: {:?}, {:?}", data, config);

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: 0,
                api_version: None,
            },
            value: None,
        })
    }

    async fn get_balance(
        &self,
        pubkey_str: String,
        config: Option<RpcContextConfig>,
    ) -> RpcResult<RpcResponse<u64>> {
        tracing::debug!("getBalance: {:?}, {:?}", pubkey_str, config);

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: 0,
                api_version: None,
            },
            value: 0,
        })
    }

    async fn get_genesis_hash(&self) -> RpcResult<String> {
        tracing::debug!("getGenesisHash");

        if !self.client.is_connected() {
            return Err(internal_error(Some("Client disconnected".to_string())));
        }

        let mut params = ArrayParams::new();
        params.insert(0).unwrap();

        let hash: Hash = self
            .client
            .request("chain_getBlockHash", params)
            .await
            .map_err(|e| internal_error(Some(e.to_string())))?;

        Ok(bs58::encode(hash.as_bytes()).into_string())
    }

    async fn get_epoch_info(&self, config: Option<RpcContextConfig>) -> RpcResult<EpochInfo> {
        tracing::debug!("getEpochInfo: {:?}", config);

        Ok(EpochInfo {
            epoch: 0,
            slot_index: 0,
            slots_in_epoch: 0,
            absolute_slot: 0,
            block_height: 0,
            transaction_count: Some(0),
        })
    }

    async fn get_transaction_count(&self, config: Option<RpcContextConfig>) -> RpcResult<u64> {
        tracing::debug!("getTransactionCount: {:?}", config);

        Ok(0)
    }
}
