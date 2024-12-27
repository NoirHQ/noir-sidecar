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

use super::SolanaServer;
use jsonrpsee::core::{async_trait, RpcResult};
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
use solana_sdk::epoch_info::EpochInfo;

pub struct MockSolana;

#[async_trait]
impl SolanaServer for MockSolana {
    async fn get_account_info(
        &self,
        pubkey_str: String,
        _config: Option<RpcAccountInfoConfig>,
    ) -> RpcResult<RpcResponse<Option<UiAccount>>> {
        tracing::debug!("get_account_info rpc request received: {:?}", pubkey_str);

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: Default::default(),
                api_version: Default::default(),
            },
            value: Default::default(),
        })
    }

    async fn get_multiple_accounts(
        &self,
        pubkey_strs: Vec<String>,
        _config: Option<RpcAccountInfoConfig>,
    ) -> RpcResult<RpcResponse<Vec<Option<UiAccount>>>> {
        tracing::debug!(
            "get_multiple_accounts rpc request received: {:?}",
            pubkey_strs.len()
        );

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: Default::default(),
                api_version: Default::default(),
            },
            value: Default::default(),
        })
    }

    async fn get_program_accounts(
        &self,
        program_id_str: String,
        _config: Option<RpcProgramAccountsConfig>,
    ) -> RpcResult<OptionalContext<Vec<RpcKeyedAccount>>> {
        tracing::debug!(
            "get_program_accounts rpc request received: {:?}",
            program_id_str
        );

        Ok(OptionalContext::NoContext(Default::default()))
    }

    async fn get_token_accounts_by_owner(
        &self,
        owner_str: String,
        _token_account_filter: RpcTokenAccountsFilter,
        _config: Option<RpcAccountInfoConfig>,
    ) -> RpcResult<RpcResponse<Vec<RpcKeyedAccount>>> {
        tracing::debug!(
            "get_token_accounts_by_owner rpc request received: {:?}",
            owner_str
        );

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: Default::default(),
                api_version: Default::default(),
            },
            value: Default::default(),
        })
    }

    async fn get_latest_blockhash(
        &self,
        _config: Option<RpcContextConfig>,
    ) -> RpcResult<RpcResponse<RpcBlockhash>> {
        tracing::debug!("get_latest_blockhash rpc request received");

        let (blockhash, slot) = get_mock_hash_and_slot();

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot,
                api_version: Default::default(),
            },
            value: RpcBlockhash {
                blockhash,
                last_valid_block_height: slot + 360000, // 1 hour
            },
        })
    }

    async fn send_transaction(
        &self,
        _data: String,
        _config: Option<RpcSendTransactionConfig>,
    ) -> RpcResult<String> {
        tracing::debug!("send_transaction rpc request received");

        Ok(Default::default())
    }

    async fn simulate_transaction(
        &self,
        _data: String,
        _config: Option<RpcSimulateTransactionConfig>,
    ) -> RpcResult<RpcResponse<RpcSimulateTransactionResult>> {
        tracing::debug!("simulate_transaction rpc request received");

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: Default::default(),
                api_version: Default::default(),
            },
            value: RpcSimulateTransactionResult {
                err: Default::default(),
                logs: Default::default(),
                accounts: Default::default(),
                units_consumed: Default::default(),
                return_data: Default::default(),
                inner_instructions: Default::default(),
                replacement_blockhash: Default::default(),
            },
        })
    }

    async fn get_inflation_reward(
        &self,
        address_strs: Vec<String>,
        _config: Option<RpcEpochConfig>,
    ) -> RpcResult<Vec<Option<RpcInflationReward>>> {
        tracing::debug!(
            "get_inflation_reward rpc request received: {:?}",
            address_strs.len()
        );

        Ok(Default::default())
    }

    async fn get_fee_for_message(
        &self,
        _data: String,
        _config: Option<RpcContextConfig>,
    ) -> RpcResult<RpcResponse<Option<u64>>> {
        tracing::debug!("get_fee_for_message rpc request received");

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: Default::default(),
                api_version: Default::default(),
            },
            value: Default::default(),
        })
    }

    async fn get_balance(
        &self,
        pubkey_str: String,
        _config: Option<RpcContextConfig>,
    ) -> RpcResult<RpcResponse<u64>> {
        tracing::debug!("get_balance rpc request received: {:?}", pubkey_str);

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: Default::default(),
                api_version: Default::default(),
            },
            value: Default::default(),
        })
    }

    async fn get_genesis_hash(&self) -> RpcResult<String> {
        tracing::debug!("get_genesis_hash rpc request received");

        Ok(Default::default())
    }

    async fn get_epoch_info(&self, _config: Option<RpcContextConfig>) -> RpcResult<EpochInfo> {
        tracing::debug!("get_epoch_info rpc request received");

        Ok(EpochInfo {
            epoch: Default::default(),
            slot_index: Default::default(),
            slots_in_epoch: Default::default(),
            absolute_slot: Default::default(),
            block_height: Default::default(),
            transaction_count: Default::default(),
        })
    }

    async fn get_transaction_count(&self, _config: Option<RpcContextConfig>) -> RpcResult<u64> {
        tracing::debug!("get_transaction_count rpc request received");

        Ok(Default::default())
    }
}

fn get_mock_hash_and_slot() -> (String, u64) {
    let slot = chrono::Utc::now().timestamp_millis() as u64;
    let mut hash = [0u8; 32];
    hash[..8].copy_from_slice(&slot.to_le_bytes());
    (bs58::encode(hash).into_string(), slot)
}
