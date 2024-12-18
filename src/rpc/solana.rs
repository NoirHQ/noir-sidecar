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
    core::{async_trait, client::ClientT, RpcResult},
    proc_macros::rpc,
    rpc_params,
    ws_client::WsClient,
};
use noir_core_primitives::{Hash, Header};
use solana_account_decoder::{encode_ui_account, UiAccount, UiAccountEncoding};
use solana_rpc_client_api::{
    config::{
        RpcAccountInfoConfig, RpcContextConfig, RpcEpochConfig, RpcProgramAccountsConfig,
        RpcSendTransactionConfig, RpcSimulateTransactionConfig, RpcTokenAccountsFilter,
    },
    filter::RpcFilterType,
    response::{
        OptionalContext, Response as RpcResponse, RpcBlockhash, RpcInflationReward,
        RpcKeyedAccount, RpcResponseContext, RpcSimulateTransactionResult,
    },
};
use solana_runtime_api::error::Error;
use solana_sdk::{
    account::Account,
    clock::Slot,
    commitment_config::{CommitmentConfig, CommitmentLevel},
    epoch_info::EpochInfo,
    pubkey::Pubkey,
};
use std::{str::FromStr, sync::Arc};

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
        tracing::debug!("get_account_info rpc request received: {:?}", pubkey_str);

        let config = config.unwrap_or_default();
        let hash = self
            .get_hash_by_context(config.commitment, config.min_context_slot)
            .await?;
        let pubkey = Pubkey::from_str(&pubkey_str).map_err(|e| parse_error(Some(e.to_string())))?;

        let method = "getAccountInfo".to_string();
        let params = serde_json::to_vec(&pubkey).map_err(|e| parse_error(Some(e.to_string())))?;

        let response = state_call::<_, Result<Vec<u8>, Error>>(
            &self.client,
            "SolanaRuntimeApi_call",
            (method, params),
            Some(hash),
        )
        .await
        .map_err(|e| internal_error(Some(e.to_string())))?
        .map_err(|e| internal_error(Some(format!("{:?}", e))))?;

        let account = serde_json::from_slice::<Option<Account>>(&response)
            .map_err(|e| internal_error(Some(e.to_string())))?;
        let encoding = config.encoding.unwrap_or(UiAccountEncoding::Binary);
        let data_slice_config = config.data_slice;

        let ui_account = account
            .map(|account| encode_ui_account(&pubkey, &account, encoding, None, data_slice_config));

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: 0,
                api_version: None,
            },
            value: ui_account,
        })
    }

    async fn get_multiple_accounts(
        &self,
        pubkey_strs: Vec<String>,
        config: Option<RpcAccountInfoConfig>,
    ) -> RpcResult<RpcResponse<Vec<Option<UiAccount>>>> {
        tracing::debug!(
            "get_multiple_accounts rpc request received: {:?}",
            pubkey_strs.len()
        );

        let config = config.unwrap_or_default();
        let hash = self
            .get_hash_by_context(config.commitment, config.min_context_slot)
            .await?;
        let pubkeys = pubkey_strs
            .iter()
            .map(|pubkey| Pubkey::from_str(pubkey))
            .collect::<Result<Vec<Pubkey>, _>>()
            .map_err(|e| parse_error(Some(e.to_string())))?;

        let method = "getMultipleAccounts".to_string();
        let params = serde_json::to_vec(&pubkeys).map_err(|e| parse_error(Some(e.to_string())))?;

        let response = state_call::<_, Result<Vec<u8>, Error>>(
            &self.client,
            "SolanaRuntimeApi_call",
            (method, params),
            Some(hash),
        )
        .await
        .map_err(|e| internal_error(Some(e.to_string())))?
        .map_err(|e| internal_error(Some(format!("{:?}", e))))?;

        let encoding = config.encoding.unwrap_or(UiAccountEncoding::Binary);
        let data_slice_config = config.data_slice;

        let accounts = serde_json::from_slice::<Vec<Option<Account>>>(&response)
            .map_err(|e| internal_error(Some(e.to_string())))?;

        if pubkey_strs.len() != accounts.len() {
            return Err(internal_error(Some(
                "Account count mismatch with public keys.".to_string(),
            )));
        }

        let ui_accounts = pubkeys
            .into_iter()
            .zip(accounts)
            .map(|(pubkey, account)| {
                account.map(|account| {
                    encode_ui_account(&pubkey, &account, encoding, None, data_slice_config)
                })
            })
            .collect::<Vec<Option<UiAccount>>>();

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: 0,
                api_version: None,
            },
            value: ui_accounts,
        })
    }

    async fn get_program_accounts(
        &self,
        program_id_str: String,
        config: Option<RpcProgramAccountsConfig>,
    ) -> RpcResult<OptionalContext<Vec<RpcKeyedAccount>>> {
        tracing::debug!(
            "get_program_accounts rpc request received: {:?}",
            program_id_str
        );

        // TODO: Handle sort_results
        let (config, mut filters, with_context, _sort_results) = if let Some(config) = config {
            (
                Some(config.account_config),
                config.filters.unwrap_or_default(),
                config.with_context.unwrap_or_default(),
                config.sort_results.unwrap_or(true),
            )
        } else {
            (None, vec![], false, true)
        };

        let RpcAccountInfoConfig {
            encoding,
            data_slice: data_slice_config,
            commitment,
            min_context_slot,
        } = config.unwrap_or_default();
        let encoding = encoding.unwrap_or(UiAccountEncoding::Binary);
        optimize_filters(&mut filters);

        let hash = self
            .get_hash_by_context(commitment, min_context_slot)
            .await?;
        let pubkey =
            Pubkey::from_str(&program_id_str).map_err(|e| parse_error(Some(e.to_string())))?;

        let accounts = get_program_accounts_by_id(&pubkey);

        let method = "getProgramAccounts".to_string();
        let params = serde_json::to_vec(&(pubkey, accounts, filters))
            .map_err(|e| parse_error(Some(e.to_string())))?;

        let response = state_call::<_, Result<Vec<u8>, Error>>(
            &self.client,
            "SolanaRuntimeApi_call",
            (method, params),
            Some(hash),
        )
        .await
        .map_err(|e| internal_error(Some(e.to_string())))?
        .map_err(|e| internal_error(Some(format!("{:?}", e))))?;

        let keyed_accounts: Vec<RpcKeyedAccount> =
            serde_json::from_slice::<Vec<(Pubkey, Account)>>(&response)
                .map_err(|e| internal_error(Some(e.to_string())))?
                .into_iter()
                .map(|(pubkey, account)| RpcKeyedAccount {
                    pubkey: pubkey.to_string(),
                    account: encode_ui_account(
                        &pubkey,
                        &account,
                        encoding,
                        None,
                        data_slice_config,
                    ),
                })
                .collect();

        Ok(match with_context {
            true => OptionalContext::Context(RpcResponse {
                context: RpcResponseContext {
                    slot: 0,
                    api_version: None,
                },
                value: keyed_accounts,
            }),
            false => OptionalContext::NoContext(keyed_accounts),
        })
    }

    async fn get_token_accounts_by_owner(
        &self,
        owner_str: String,
        token_account_filter: RpcTokenAccountsFilter,
        config: Option<RpcAccountInfoConfig>,
    ) -> RpcResult<RpcResponse<Vec<RpcKeyedAccount>>> {
        tracing::debug!(
            "get_token_accounts_by_owner rpc request received: {:?}",
            owner_str
        );

        let method = "getTokenAccountsByOwner".to_string();
        let params = serde_json::to_vec(&(owner_str, token_account_filter, config))
            .map_err(|e| parse_error(Some(e.to_string())))?;

        let response = state_call::<_, Result<Vec<u8>, Error>>(
            &self.client,
            "SolanaRuntimeApi_call",
            (method, params),
            None,
        )
        .await
        .map_err(|e| internal_error(Some(e.to_string())))?
        .map_err(|e| internal_error(Some(format!("{:?}", e))))?;

        serde_json::from_slice::<_>(&response).map_err(|e| internal_error(Some(e.to_string())))
    }

    async fn get_latest_blockhash(
        &self,
        config: Option<RpcContextConfig>,
    ) -> RpcResult<RpcResponse<RpcBlockhash>> {
        tracing::debug!("get_latest_blockhash rpc request received");

        let config = config.unwrap_or_default();
        let hash = self
            .get_hash_by_context(config.commitment, config.min_context_slot)
            .await?;

        let Header { number, .. } = self
            .client
            .request("chain_getHeader", rpc_params!(hash))
            .await
            .map_err(|e| internal_error(Some(e.to_string())))?;

        // TODO: Complete rpc response context
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
        tracing::debug!("send_transaction rpc request received");

        let method = "sendTransaction".to_string();
        let params =
            serde_json::to_vec(&(data, config)).map_err(|e| parse_error(Some(e.to_string())))?;

        let response = state_call::<_, Result<Vec<u8>, Error>>(
            &self.client,
            "SolanaRuntimeApi_call",
            (method, params),
            None,
        )
        .await
        .map_err(|e| internal_error(Some(e.to_string())))?
        .map_err(|e| internal_error(Some(format!("{:?}", e))))?;

        serde_json::from_slice::<_>(&response).map_err(|e| internal_error(Some(e.to_string())))
    }

    async fn simulate_transaction(
        &self,
        data: String,
        config: Option<RpcSimulateTransactionConfig>,
    ) -> RpcResult<RpcResponse<RpcSimulateTransactionResult>> {
        tracing::debug!("simulate_transaction rpc request received");

        let method = "simulateTransaction".to_string();
        let params =
            serde_json::to_vec(&(data, config)).map_err(|e| parse_error(Some(e.to_string())))?;

        let response = state_call::<_, Result<Vec<u8>, Error>>(
            &self.client,
            "SolanaRuntimeApi_call",
            (method, params),
            None,
        )
        .await
        .map_err(|e| internal_error(Some(e.to_string())))?
        .map_err(|e| internal_error(Some(format!("{:?}", e))))?;

        serde_json::from_slice::<_>(&response).map_err(|e| internal_error(Some(e.to_string())))
    }

    async fn get_inflation_reward(
        &self,
        address_strs: Vec<String>,
        config: Option<RpcEpochConfig>,
    ) -> RpcResult<Vec<Option<RpcInflationReward>>> {
        tracing::debug!(
            "get_inflation_reward rpc request received: {:?}",
            address_strs.len()
        );

        let method = "getInflationReward".to_string();
        let params = serde_json::to_vec(&(address_strs, config))
            .map_err(|e| parse_error(Some(e.to_string())))?;

        let response = state_call::<_, Result<Vec<u8>, Error>>(
            &self.client,
            "SolanaRuntimeApi_call",
            (method, params),
            None,
        )
        .await
        .map_err(|e| internal_error(Some(e.to_string())))?
        .map_err(|e| internal_error(Some(format!("{:?}", e))))?;

        serde_json::from_slice::<_>(&response).map_err(|e| internal_error(Some(e.to_string())))
    }

    async fn get_fee_for_message(
        &self,
        data: String,
        config: Option<RpcContextConfig>,
    ) -> RpcResult<RpcResponse<Option<u64>>> {
        tracing::debug!("get_fee_for_message rpc request received");

        let method = "getFeeForMessage".to_string();
        let params =
            serde_json::to_vec(&(data, config)).map_err(|e| parse_error(Some(e.to_string())))?;

        let response = state_call::<_, Result<Vec<u8>, Error>>(
            &self.client,
            "SolanaRuntimeApi_call",
            (method, params),
            None,
        )
        .await
        .map_err(|e| internal_error(Some(e.to_string())))?
        .map_err(|e| internal_error(Some(format!("{:?}", e))))?;

        serde_json::from_slice::<_>(&response).map_err(|e| internal_error(Some(e.to_string())))
    }

    async fn get_balance(
        &self,
        pubkey_str: String,
        config: Option<RpcContextConfig>,
    ) -> RpcResult<RpcResponse<u64>> {
        tracing::debug!("get_balance rpc request received: {:?}", pubkey_str);

        let config = config.unwrap_or_default();
        let pubkey = Pubkey::from_str(&pubkey_str).map_err(|e| parse_error(Some(e.to_string())))?;
        let hash = self
            .get_hash_by_context(config.commitment, config.min_context_slot)
            .await?;

        let method = "getBalance".to_string();
        let params = serde_json::to_vec(&pubkey).map_err(|e| parse_error(Some(e.to_string())))?;

        let response = state_call::<_, Result<Vec<u8>, Error>>(
            &self.client,
            "SolanaRuntimeApi_call",
            (method, params),
            Some(hash),
        )
        .await
        .map_err(|e| internal_error(Some(e.to_string())))?
        .map_err(|e| internal_error(Some(format!("{:?}", e))))?;

        let balance = serde_json::from_slice::<u64>(&response)
            .map_err(|e| internal_error(Some(e.to_string())))?;

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: 0,
                api_version: None,
            },
            value: balance,
        })
    }

    async fn get_genesis_hash(&self) -> RpcResult<String> {
        tracing::debug!("get_genesis_hash rpc request received");

        if !self.client.is_connected() {
            return Err(internal_error(Some("Client disconnected".to_string())));
        }

        let hash: Hash = self
            .client
            .request("chain_getBlockHash", rpc_params!(0))
            .await
            .map_err(|e| internal_error(Some(e.to_string())))?;

        Ok(bs58::encode(hash.as_bytes()).into_string())
    }

    async fn get_epoch_info(&self, config: Option<RpcContextConfig>) -> RpcResult<EpochInfo> {
        tracing::debug!("get_epoch_info rpc request received");

        let method = "getEpochInfo".to_string();
        let params = serde_json::to_vec(&config).map_err(|e| parse_error(Some(e.to_string())))?;

        let response = state_call::<_, Result<Vec<u8>, Error>>(
            &self.client,
            "SolanaRuntimeApi_call",
            (method, params),
            None,
        )
        .await
        .map_err(|e| internal_error(Some(e.to_string())))?
        .map_err(|e| internal_error(Some(format!("{:?}", e))))?;

        serde_json::from_slice::<_>(&response).map_err(|e| internal_error(Some(e.to_string())))
    }

    async fn get_transaction_count(&self, config: Option<RpcContextConfig>) -> RpcResult<u64> {
        tracing::debug!("get_transaction_count rpc request received");

        let method = "getTransactionCount".to_string();
        let params = serde_json::to_vec(&config).map_err(|e| parse_error(Some(e.to_string())))?;

        let response = state_call::<_, Result<Vec<u8>, Error>>(
            &self.client,
            "SolanaRuntimeApi_call",
            (method, params),
            None,
        )
        .await
        .map_err(|e| internal_error(Some(e.to_string())))?
        .map_err(|e| internal_error(Some(format!("{:?}", e))))?;

        serde_json::from_slice::<_>(&response).map_err(|e| internal_error(Some(e.to_string())))
    }
}

impl Solana {
    async fn get_hash_by_context(
        &self,
        commitment: Option<CommitmentConfig>,
        min_context_slot: Option<Slot>,
    ) -> RpcResult<Hash> {
        if let Some(_min_context_slot) = min_context_slot {
            // TODO: Handle min_context_slot
        }

        if !self.client.is_connected() {
            return Err(internal_error(Some("Client disconnected".to_string())));
        }

        let commitment = commitment.unwrap_or_default();
        let hash: Hash = match commitment.commitment {
            CommitmentLevel::Processed => {
                self.client
                    .request("chain_getBlockHash", rpc_params!())
                    .await
            }
            CommitmentLevel::Confirmed | CommitmentLevel::Finalized => {
                self.client
                    .request("chain_getFinalizedHead", rpc_params!())
                    .await
            }
        }
        .map_err(|e| internal_error(Some(e.to_string())))?;

        Ok(hash)
    }
}

fn optimize_filters(filters: &mut [RpcFilterType]) {
    filters.iter_mut().for_each(|filter_type| {
        if let RpcFilterType::Memcmp(compare) = filter_type {
            if let Err(err) = compare.convert_to_raw_bytes() {
                // All filters should have been previously verified
                tracing::warn!("Invalid filter: bytes could not be decoded, {err}");
            }
        }
    })
}

fn get_program_accounts_by_id(_program_id: &Pubkey) -> Vec<Pubkey> {
    // TODO: Get accounts owned by program
    Vec::new()
}
