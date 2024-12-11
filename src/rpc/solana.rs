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

use crate::client::Client;
use jsonrpc_core::{BoxFuture, Result};
use jsonrpc_derive::rpc;
use serde::{Deserialize, Serialize};
use solana_account_decoder::UiAccount;
use solana_rpc_client_api::{
    config::*,
    response::{Response as RpcResponse, *},
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpochInfo {
    /// The current epoch
    pub epoch: u64,

    /// The current slot, relative to the start of the current epoch
    pub slot_index: u64,

    /// The number of slots in this epoch
    pub slots_in_epoch: u64,

    /// The absolute current slot
    pub absolute_slot: u64,

    /// The current block height
    pub block_height: u64,

    /// Total number of transactions processed without error since genesis
    pub transaction_count: Option<u64>,
}

#[rpc]
pub trait Solana {
    type Metadata;

    #[rpc(meta, name = "getAccountInfo")]
    fn get_account_info(
        &self,
        meta: Self::Metadata,
        pubkey_str: String,
        config: Option<solana_rpc_client_api::config::RpcAccountInfoConfig>,
    ) -> Result<RpcResponse<Option<UiAccount>>>;

    #[rpc(meta, name = "getMultipleAccounts")]
    fn get_multiple_accounts(
        &self,
        meta: Self::Metadata,
        pubkey_strs: Vec<String>,
        config: Option<RpcAccountInfoConfig>,
    ) -> Result<RpcResponse<Vec<Option<UiAccount>>>>;

    #[rpc(meta, name = "getProgramAccounts")]
    fn get_program_accounts(
        &self,
        meta: Self::Metadata,
        program_id_str: String,
        config: Option<RpcProgramAccountsConfig>,
    ) -> Result<OptionalContext<Vec<RpcKeyedAccount>>>;

    #[rpc(meta, name = "getTokenAccountsByOwner")]
    fn get_token_accounts_by_owner(
        &self,
        meta: Self::Metadata,
        owner_str: String,
        token_account_filter: RpcTokenAccountsFilter,
        config: Option<RpcAccountInfoConfig>,
    ) -> Result<RpcResponse<Vec<RpcKeyedAccount>>>;

    #[rpc(meta, name = "getLatestBlockhash")]
    fn get_latest_blockhash(
        &self,
        meta: Self::Metadata,
        config: Option<RpcContextConfig>,
    ) -> Result<RpcResponse<RpcBlockhash>>;

    #[rpc(meta, name = "sendTransaction")]
    fn send_transaction(
        &self,
        meta: Self::Metadata,
        data: String,
        config: Option<RpcSendTransactionConfig>,
    ) -> Result<String>;

    #[rpc(meta, name = "getInflationReward")]
    fn get_inflation_reward(
        &self,
        meta: Self::Metadata,
        address_strs: Vec<String>,
        config: Option<RpcEpochConfig>,
    ) -> BoxFuture<Result<Vec<Option<RpcInflationReward>>>>;

    #[rpc(meta, name = "getFeeForMessage")]
    fn get_fee_for_message(
        &self,
        meta: Self::Metadata,
        data: String,
        config: Option<RpcContextConfig>,
    ) -> Result<RpcResponse<Option<u64>>>;

    #[rpc(meta, name = "getBalance")]
    fn get_balance(
        &self,
        meta: Self::Metadata,
        pubkey_str: String,
        config: Option<RpcContextConfig>,
    ) -> Result<RpcResponse<u64>>;

    #[rpc(meta, name = "getEpochInfo")]
    fn get_epoch_info(
        &self,
        meta: Self::Metadata,
        config: Option<RpcContextConfig>,
    ) -> Result<EpochInfo>;

    #[rpc(meta, name = "getGenesisHash")]
    fn get_genesis_hash(&self, meta: Self::Metadata) -> Result<String>;

    #[rpc(meta, name = "getTransactionCount")]
    fn get_transaction_count(
        &self,
        meta: Self::Metadata,
        config: Option<RpcContextConfig>,
    ) -> Result<u64>;
}

pub struct SolanaImpl;
impl Solana for SolanaImpl {
    type Metadata = Client;

    fn get_account_info(
        &self,
        _meta: Self::Metadata,
        pubkey_str: String,
        config: Option<RpcAccountInfoConfig>,
    ) -> Result<RpcResponse<Option<UiAccount>>> {
        tracing::debug!("get_account_info: {:?}, {:?}", pubkey_str, config);

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: 0,
                api_version: None,
            },
            value: None,
        })
    }

    fn get_multiple_accounts(
        &self,
        _meta: Self::Metadata,
        pubkey_strs: Vec<String>,
        config: Option<RpcAccountInfoConfig>,
    ) -> Result<RpcResponse<Vec<Option<UiAccount>>>> {
        tracing::debug!("get_multiple_accounts: {:?}, {:?}", pubkey_strs, config);

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: 0,
                api_version: None,
            },
            value: Vec::default(),
        })
    }

    fn get_program_accounts(
        &self,
        _meta: Self::Metadata,
        program_id_str: String,
        config: Option<RpcProgramAccountsConfig>,
    ) -> Result<OptionalContext<Vec<RpcKeyedAccount>>> {
        tracing::debug!("get_program_accounts: {:?}, {:?}", program_id_str, config);

        Ok(OptionalContext::NoContext(Vec::default()))
    }

    fn get_token_accounts_by_owner(
        &self,
        _meta: Self::Metadata,
        owner_str: String,
        token_account_filter: RpcTokenAccountsFilter,
        config: Option<RpcAccountInfoConfig>,
    ) -> Result<RpcResponse<Vec<RpcKeyedAccount>>> {
        tracing::debug!(
            "get_token_accounts_by_owner: {:?}, {:?}, {:?}",
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

    fn get_latest_blockhash(
        &self,
        _meta: Self::Metadata,
        config: Option<RpcContextConfig>,
    ) -> Result<RpcResponse<RpcBlockhash>> {
        tracing::debug!("get_latest_blockhash: {:?}", config);

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: 0,
                api_version: None,
            },
            value: RpcBlockhash {
                blockhash: String::default(),
                last_valid_block_height: 0,
            },
        })
    }

    fn send_transaction(
        &self,
        _meta: Self::Metadata,
        data: String,
        config: Option<RpcSendTransactionConfig>,
    ) -> Result<String> {
        tracing::debug!("send_transaction: {:?}, {:?}", data, config);

        Ok(String::default())
    }

    fn get_inflation_reward(
        &self,
        _meta: Self::Metadata,
        address_strs: Vec<String>,
        config: Option<RpcEpochConfig>,
    ) -> BoxFuture<Result<Vec<Option<RpcInflationReward>>>> {
        tracing::debug!("get_inflation_reward: {:?}, {:?}", address_strs, config);

        Box::pin(async move { Ok(Vec::default()) })
    }

    fn get_fee_for_message(
        &self,
        _meta: Self::Metadata,
        data: String,
        config: Option<RpcContextConfig>,
    ) -> Result<RpcResponse<Option<u64>>> {
        tracing::debug!("get_fee_for_message: {:?}, {:?}", data, config);

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: 0,
                api_version: None,
            },
            value: None,
        })
    }

    fn get_balance(
        &self,
        _meta: Self::Metadata,
        pubkey_str: String,
        config: Option<RpcContextConfig>,
    ) -> Result<RpcResponse<u64>> {
        tracing::debug!("get_balance: {:?}, {:?}", pubkey_str, config);

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: 0,
                api_version: None,
            },
            value: 0,
        })
    }

    fn get_epoch_info(
        &self,
        _meta: Self::Metadata,
        config: Option<RpcContextConfig>,
    ) -> Result<EpochInfo> {
        tracing::debug!("get_epoch_info: {:?}", config);

        Ok(EpochInfo {
            epoch: 0,
            slot_index: 0,
            slots_in_epoch: 0,
            absolute_slot: 0,
            block_height: 0,
            transaction_count: Some(0),
        })
    }

    fn get_genesis_hash(&self, _meta: Self::Metadata) -> Result<String> {
        tracing::debug!("get_genesis_hash");

        Ok(String::default())
    }

    fn get_transaction_count(
        &self,
        _meta: Self::Metadata,
        config: Option<RpcContextConfig>,
    ) -> Result<u64> {
        tracing::debug!("get_transaction_count: {:?}", config);

        Ok(0)
    }
}
