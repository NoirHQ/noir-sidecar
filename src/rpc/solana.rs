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

use crate::rpc::{internal_error, invalid_params, parse_error, state_call};
use jsonrpsee::{
    core::{async_trait, client::ClientT, RpcResult},
    proc_macros::rpc,
    rpc_params,
    ws_client::WsClient,
};
use noir_core_primitives::{Hash, Header};
use solana_account_decoder::{
    encode_ui_account,
    parse_account_data::{AccountAdditionalDataV2, SplTokenAdditionalData},
    parse_token::{get_token_account_mint, is_known_spl_token_id},
    UiAccount, UiAccountEncoding, UiDataSliceConfig, MAX_BASE58_BYTES,
};
use solana_inline_spl::token::SPL_TOKEN_ACCOUNT_OWNER_OFFSET;
use solana_rpc_client_api::{
    config::{
        RpcAccountInfoConfig, RpcContextConfig, RpcEpochConfig, RpcProgramAccountsConfig,
        RpcSendTransactionConfig, RpcSimulateTransactionConfig, RpcTokenAccountsFilter,
    },
    filter::{Memcmp, RpcFilterType},
    request::{TokenAccountsFilter, MAX_GET_PROGRAM_ACCOUNT_FILTERS, MAX_MULTIPLE_ACCOUNTS},
    response::{
        OptionalContext, Response as RpcResponse, RpcBlockhash, RpcInflationReward,
        RpcKeyedAccount, RpcResponseContext, RpcSimulateTransactionResult,
    },
};
use solana_runtime_api::error::Error;
use solana_sdk::{
    account::{Account, ReadableAccount},
    clock::{Slot, UnixTimestamp},
    commitment_config::{CommitmentConfig, CommitmentLevel},
    epoch_info::EpochInfo,
    pubkey::Pubkey,
};
use spl_token_2022::{
    extension::{
        interest_bearing_mint::InterestBearingConfig, BaseStateWithExtensions, StateWithExtensions,
    },
    state::Mint,
};
use std::{cmp::min, collections::HashMap, str::FromStr, sync::Arc};

use super::invalid_request;

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

        let pubkey = verify_pubkey(&pubkey_str)?;
        let RpcAccountInfoConfig {
            encoding,
            data_slice: data_slice_config,
            commitment,
            min_context_slot,
        } = config.unwrap_or_default();
        let hash = self
            .get_hash_by_context(commitment, min_context_slot)
            .await?;
        let encoding = encoding.unwrap_or(UiAccountEncoding::Binary);

        let response = self
            .get_encoded_account(&pubkey, encoding, data_slice_config, hash)
            .await?;
        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: 0,
                api_version: None,
            },
            value: response,
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

        if pubkey_strs.len() > MAX_MULTIPLE_ACCOUNTS {
            return Err(invalid_params(Some(format!(
                "Too many inputs provided; max {MAX_MULTIPLE_ACCOUNTS}"
            ))));
        }
        let pubkeys = pubkey_strs
            .iter()
            .map(|pubkey| verify_pubkey(pubkey))
            .collect::<Result<Vec<Pubkey>, _>>()
            .map_err(|e| invalid_params(Some(e.to_string())))?;

        let RpcAccountInfoConfig {
            encoding,
            data_slice: data_slice_config,
            commitment,
            min_context_slot,
        } = config.unwrap_or_default();
        let hash = self
            .get_hash_by_context(commitment, min_context_slot)
            .await?;
        let encoding = encoding.unwrap_or(UiAccountEncoding::Binary);

        let response = self
            .get_multiple_encoded_accounts(&pubkeys, encoding, data_slice_config, hash)
            .await?;

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: 0,
                api_version: None,
            },
            value: response,
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
        if filters.len() > MAX_GET_PROGRAM_ACCOUNT_FILTERS {
            return Err(invalid_params(Some(format!(
                "Too many filters provided; max {MAX_GET_PROGRAM_ACCOUNT_FILTERS}"
            ))));
        }
        for filter in &filters {
            verify_filter(filter)?;
        }

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
        let program_id = verify_pubkey(&program_id_str)?;
        let accounts = self.get_program_accounts_by_id(&program_id);

        let method = "getProgramAccounts".to_string();
        let params = serde_json::to_vec(&(program_id, accounts, filters))
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

        let RpcAccountInfoConfig {
            encoding,
            data_slice: data_slice_config,
            commitment,
            min_context_slot,
        } = config.unwrap_or_default();
        let encoding = encoding.unwrap_or(UiAccountEncoding::Binary);
        let hash = self
            .get_hash_by_context(commitment, min_context_slot)
            .await?;

        let owner = verify_pubkey(&owner_str)?;
        let token_account_filter = verify_token_account_filter(token_account_filter)?;
        let (token_program_id, mint) = self
            .get_token_program_id_and_mint(token_account_filter, hash)
            .await?;

        let mut filters = vec![];
        if let Some(mint) = mint {
            // Optional filter on Mint address
            filters.push(RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
                0,
                mint.to_bytes().into(),
            )));
        }

        filters.push(RpcFilterType::TokenAccountState);
        filters.push(RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            SPL_TOKEN_ACCOUNT_OWNER_OFFSET,
            owner.to_bytes().into(),
        )));

        let method = "getProgramAccounts".to_string();
        let accounts = self.get_program_accounts_by_id(&token_program_id);
        let params = serde_json::to_vec(&(token_program_id, accounts, filters))
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

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: 0,
                api_version: None,
            },
            value: keyed_accounts,
        })
    }

    async fn get_latest_blockhash(
        &self,
        config: Option<RpcContextConfig>,
    ) -> RpcResult<RpcResponse<RpcBlockhash>> {
        tracing::debug!("get_latest_blockhash rpc request received");

        let RpcContextConfig {
            commitment,
            min_context_slot,
        } = config.unwrap_or_default();
        let hash = self
            .get_hash_by_context(commitment, min_context_slot)
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
        _config: Option<RpcEpochConfig>,
    ) -> RpcResult<Vec<Option<RpcInflationReward>>> {
        tracing::debug!(
            "get_inflation_reward rpc request received: {:?}",
            address_strs.len()
        );

        Ok(address_strs
            .into_iter()
            .map(|_| None)
            .collect::<Vec<Option<RpcInflationReward>>>())
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

        let RpcContextConfig {
            commitment,
            min_context_slot,
        } = config.unwrap_or_default();
        let pubkey = verify_pubkey(&pubkey_str)?;
        let hash = self
            .get_hash_by_context(commitment, min_context_slot)
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

        let RpcContextConfig {
            commitment,
            min_context_slot,
        } = config.unwrap_or_default();
        let hash = self
            .get_hash_by_context(commitment, min_context_slot)
            .await?;

        let method = "getTransactionCount".to_string();
        let params = serde_json::to_vec("").map_err(|e| parse_error(Some(e.to_string())))?;

        let response = state_call::<_, Result<Vec<u8>, Error>>(
            &self.client,
            "SolanaRuntimeApi_call",
            (method, params),
            Some(hash),
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

    /// Analyze a passed Pubkey that may be a Token program id or Mint address to determine the program
    /// id and optional Mint
    async fn get_token_program_id_and_mint(
        &self,
        token_account_filter: TokenAccountsFilter,
        hash: Hash,
    ) -> RpcResult<(Pubkey, Option<Pubkey>)> {
        match token_account_filter {
            TokenAccountsFilter::Mint(mint) => {
                let (mint_owner, _) = self.get_mint_owner_and_additional_data(&mint, hash).await?;
                if !is_known_spl_token_id(&mint_owner) {
                    return Err(invalid_params(Some(
                        "Invalid param: not a Token mint".to_string(),
                    )));
                }
                Ok((mint_owner, Some(mint)))
            }
            TokenAccountsFilter::ProgramId(program_id) => {
                if is_known_spl_token_id(&program_id) {
                    Ok((program_id, None))
                } else {
                    Err(invalid_params(Some(
                        "Invalid param: unrecognized Token program id".to_string(),
                    )))
                }
            }
        }
    }

    /// Analyze a mint Pubkey that may be the native_mint and get the mint-account owner (token
    /// program_id) and decimals
    pub async fn get_mint_owner_and_additional_data(
        &self,
        mint: &Pubkey,
        hash: Hash,
    ) -> RpcResult<(Pubkey, SplTokenAdditionalData)> {
        if mint == &spl_token::native_mint::id() {
            Ok((
                spl_token::id(),
                SplTokenAdditionalData::with_decimals(spl_token::native_mint::DECIMALS),
            ))
        } else {
            let mint_account = self
                .get_account(mint, hash)
                .await?
                .ok_or(invalid_params(Some(
                    "Invalid param: could not find mint".to_string(),
                )))?;
            let timestamp = self.get_timestamp(hash).await?;
            let mint_data = get_additional_mint_data(mint_account.data(), timestamp)?;
            Ok((*mint_account.owner(), mint_data))
        }
    }

    fn get_program_accounts_by_id(&self, _program_id: &Pubkey) -> Vec<Pubkey> {
        // TODO: Get accounts owned by program
        Vec::new()
    }

    pub async fn get_encoded_account(
        &self,
        pubkey: &Pubkey,
        encoding: UiAccountEncoding,
        data_slice: Option<UiDataSliceConfig>,
        hash: Hash,
    ) -> RpcResult<Option<UiAccount>> {
        match self.get_account(pubkey, hash).await? {
            Some(account) => {
                let response = if is_known_spl_token_id(account.owner())
                    && encoding == UiAccountEncoding::JsonParsed
                {
                    self.get_parsed_token_account(pubkey, account, hash).await?
                } else {
                    encode_account(&account, pubkey, encoding, data_slice)?
                };
                Ok(Some(response))
            }
            None => Ok(None),
        }
    }

    pub async fn get_multiple_encoded_accounts(
        &self,
        pubkeys: &Vec<Pubkey>,
        encoding: UiAccountEncoding,
        data_slice: Option<UiDataSliceConfig>,
        hash: Hash,
    ) -> RpcResult<Vec<Option<UiAccount>>> {
        let accounts = self.get_accounts(pubkeys, hash).await?;
        let spl_token_ids = filter_known_spl_token_id(pubkeys.clone());

        if encoding == UiAccountEncoding::JsonParsed && !spl_token_ids.is_empty() {
            self.get_multiple_parsed_token_accounts(accounts, spl_token_ids, hash)
                .await
        } else {
            let ui_accounts = accounts
                .into_iter()
                .map(|(pubkey, account)| match account {
                    Some(account) => Some(encode_ui_account(
                        &pubkey, &account, encoding, None, data_slice,
                    )),
                    None => None,
                })
                .collect::<Vec<Option<UiAccount>>>();
            Ok(ui_accounts)
        }
    }

    pub async fn get_parsed_token_account(
        &self,
        pubkey: &Pubkey,
        account: Account,
        hash: Hash,
    ) -> RpcResult<UiAccount> {
        let additional_data = if let Some(mint_pubkey) = get_token_account_mint(account.data()) {
            match self.get_account(&mint_pubkey, hash).await? {
                Some(mint_account) => {
                    let timestamp = self.get_timestamp(hash).await?;
                    let data = get_additional_mint_data(mint_account.data(), timestamp)?;
                    Some(AccountAdditionalDataV2 {
                        spl_token_additional_data: Some(data),
                    })
                }
                None => None,
            }
        } else {
            None
        };

        Ok(encode_ui_account(
            pubkey,
            &account,
            UiAccountEncoding::JsonParsed,
            additional_data,
            None,
        ))
    }

    pub async fn get_multiple_parsed_token_accounts(
        &self,
        accounts: Vec<(Pubkey, Option<Account>)>,
        spl_token_ids: Vec<Pubkey>,
        hash: Hash,
    ) -> RpcResult<Vec<Option<UiAccount>>> {
        let mint_pubkeys = get_multiple_token_account_mint(&accounts, &spl_token_ids);
        let mint_accounts: HashMap<Pubkey, Account> = self
            .get_accounts(
                &mint_pubkeys.values().cloned().collect::<Vec<Pubkey>>(),
                hash,
            )
            .await?
            .into_iter()
            .filter_map(|(mint_pubkey, mint_account)| {
                mint_account.map(|account| (mint_pubkey, account))
            })
            .collect();

        let timestamp = self.get_timestamp(hash).await?;
        let account_additional_data = get_multiple_additional_mint_data(&mint_accounts, timestamp);

        let ui_accounts = accounts
            .into_iter()
            .map(|(pubkey, account)| match account {
                Some(account) => {
                    let additional_data = mint_pubkeys
                        .get(&pubkey)
                        .map(|mint_pubkey| account_additional_data.get(mint_pubkey))
                        .flatten()
                        .cloned();

                    Some(encode_ui_account(
                        &pubkey,
                        &account,
                        UiAccountEncoding::JsonParsed,
                        additional_data,
                        None,
                    ))
                }
                None => None,
            })
            .collect();

        Ok(ui_accounts)
    }

    pub async fn get_account(&self, pubkey: &Pubkey, hash: Hash) -> RpcResult<Option<Account>> {
        let method = "getAccountInfo".to_string();
        let params = serde_json::to_vec(pubkey).map_err(|e| parse_error(Some(e.to_string())))?;

        let response = state_call::<_, Result<Vec<u8>, Error>>(
            &self.client,
            "SolanaRuntimeApi_call",
            (method, params),
            Some(hash),
        )
        .await
        .map_err(|e| internal_error(Some(e.to_string())))?
        .map_err(|e| internal_error(Some(format!("{:?}", e))))?;

        serde_json::from_slice::<Option<Account>>(&response)
            .map_err(|e| internal_error(Some(e.to_string())))
    }

    pub async fn get_accounts(
        &self,
        pubkeys: &Vec<Pubkey>,
        hash: Hash,
    ) -> RpcResult<Vec<(Pubkey, Option<Account>)>> {
        let method = "getMultipleAccounts".to_string();
        let params = serde_json::to_vec(pubkeys).map_err(|e| parse_error(Some(e.to_string())))?;

        let response = state_call::<_, Result<Vec<u8>, Error>>(
            &self.client,
            "SolanaRuntimeApi_call",
            (method, params),
            Some(hash),
        )
        .await
        .map_err(|e| internal_error(Some(e.to_string())))?
        .map_err(|e| internal_error(Some(format!("{:?}", e))))?;

        let accounts = serde_json::from_slice::<Vec<Option<Account>>>(&response)
            .map_err(|e| internal_error(Some(e.to_string())))?;

        if pubkeys.len() != accounts.len() {
            return Err(internal_error(Some(
                "Account count mismatch with public keys.".to_string(),
            )));
        }

        Ok(pubkeys.iter().cloned().zip(accounts).collect())
    }

    pub async fn get_timestamp(&self, hash: Hash) -> RpcResult<UnixTimestamp> {
        let timestamp_key = "0xf0c365c3cf59d671eb72da0e7a4113c49f1f0515f462cdcf84e0f1d6045dfcbb";

        if !self.client.is_connected() {
            return Err(internal_error(Some("Client disconnected")));
        }

        let mut response: String = self
            .client
            .request::<Option<String>, _>("state_getStorage", rpc_params!(timestamp_key, hash))
            .await
            .map_err(|e| internal_error(Some(e.to_string())))?
            .ok_or(internal_error(Some("Timestamp not exist")))?;

        if response.starts_with("0x") {
            response = response.strip_prefix("0x").map(|s| s.to_string()).unwrap();
        }
        let response = hex::decode(response).map_err(|e| internal_error(Some(e.to_string())))?;

        Ok(u64::from_le_bytes(response.try_into().unwrap()) as i64)
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

fn verify_filter(input: &RpcFilterType) -> RpcResult<()> {
    input
        .verify()
        .map_err(|e| invalid_params(Some(format!("Invalid param: {e:?}"))))
}

pub fn verify_pubkey(pubkey: &str) -> RpcResult<Pubkey> {
    Pubkey::from_str(pubkey).map_err(|e| invalid_params(Some(format!("Invalid param: {e:?}"))))
}

fn verify_token_account_filter(
    token_account_filter: RpcTokenAccountsFilter,
) -> RpcResult<TokenAccountsFilter> {
    match token_account_filter {
        RpcTokenAccountsFilter::Mint(mint_str) => {
            let mint = verify_pubkey(&mint_str)?;
            Ok(TokenAccountsFilter::Mint(mint))
        }
        RpcTokenAccountsFilter::ProgramId(program_id_str) => {
            let program_id = verify_pubkey(&program_id_str)?;
            Ok(TokenAccountsFilter::ProgramId(program_id))
        }
    }
}

fn encode_account<T: ReadableAccount>(
    account: &T,
    pubkey: &Pubkey,
    encoding: UiAccountEncoding,
    data_slice: Option<UiDataSliceConfig>,
) -> RpcResult<UiAccount> {
    if (encoding == UiAccountEncoding::Binary || encoding == UiAccountEncoding::Base58)
        && data_slice
            .map(|s| min(s.length, account.data().len().saturating_sub(s.offset)))
            .unwrap_or(account.data().len())
            > MAX_BASE58_BYTES
    {
        let message = format!("Encoded binary (base 58) data should be less than {MAX_BASE58_BYTES} bytes, please use Base64 encoding.");
        Err(invalid_request(Some(message)))
    } else {
        Ok(encode_ui_account(
            pubkey, account, encoding, None, data_slice,
        ))
    }
}

fn get_additional_mint_data(
    data: &[u8],
    timestamp: UnixTimestamp,
) -> RpcResult<SplTokenAdditionalData> {
    StateWithExtensions::<Mint>::unpack(data)
        .map_err(|e| invalid_params(Some(e.to_string())))
        .map(|mint| {
            let interest_bearing_config = mint
                .get_extension::<InterestBearingConfig>()
                .map(|x| (*x, timestamp))
                .ok();
            SplTokenAdditionalData {
                decimals: mint.base.decimals,
                interest_bearing_config,
            }
        })
}

fn get_multiple_additional_mint_data(
    accounts: &HashMap<Pubkey, Account>,
    timestamp: UnixTimestamp,
) -> HashMap<Pubkey, AccountAdditionalDataV2> {
    accounts
        .iter()
        .filter_map(|(pubkey, account)| {
            get_additional_mint_data(&account.data, timestamp)
                .ok()
                .map(|additional_data| {
                    (
                        pubkey.clone(),
                        AccountAdditionalDataV2 {
                            spl_token_additional_data: Some(additional_data),
                        },
                    )
                })
        })
        .collect()
}

pub fn filter_known_spl_token_id(program_ids: Vec<Pubkey>) -> Vec<Pubkey> {
    program_ids
        .into_iter()
        .filter(|program_id| is_known_spl_token_id(program_id))
        .collect()
}

pub fn get_multiple_token_account_mint(
    accounts: &[(Pubkey, Option<Account>)],
    spl_token_ids: &[Pubkey],
) -> HashMap<Pubkey, Pubkey> {
    accounts
        .iter()
        .filter_map(|(pubkey, account)| {
            account.as_ref().and_then(|acc| {
                if spl_token_ids.contains(pubkey) {
                    get_token_account_mint(&acc.data)
                        .map(|mint_pubkey| (pubkey.clone(), mint_pubkey))
                } else {
                    None
                }
            })
        })
        .collect()
}
