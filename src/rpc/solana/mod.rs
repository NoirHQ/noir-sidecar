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

#![allow(clippy::type_complexity)]

#[cfg(feature = "mock")]
pub mod mock;

use super::invalid_request;
use crate::client::Client;
use crate::db::index::traits;
use crate::rpc::{internal_error, invalid_params, parse_error, state_call};
use base64::{prelude::BASE64_STANDARD, Engine};
use bincode::Options;
use jsonrpsee::core::params::ArrayParams;
use jsonrpsee::{
    core::{async_trait, RpcResult},
    proc_macros::rpc,
    rpc_params,
};
use noir_core_primitives::{Hash, Header};
use serde::{Deserialize, Serialize};
use solana_account_decoder::{
    encode_ui_account,
    parse_account_data::{AccountAdditionalDataV2, SplTokenAdditionalData},
    parse_token::{get_token_account_mint, is_known_spl_token_id},
    UiAccount, UiAccountData, UiAccountEncoding, UiDataSliceConfig, MAX_BASE58_BYTES,
};
use solana_accounts_db::accounts_index::AccountIndex;
use solana_inline_spl::{
    token::{SPL_TOKEN_ACCOUNT_MINT_OFFSET, SPL_TOKEN_ACCOUNT_OWNER_OFFSET},
    token_2022::{self, ACCOUNTTYPE_ACCOUNT},
};
use solana_rpc_client_api::config::{
    RpcGetVoteAccountsConfig, RpcRequestAirdropConfig, RpcSignatureStatusConfig,
    RpcSignaturesForAddressConfig,
};
use solana_rpc_client_api::request::MAX_GET_SIGNATURE_STATUSES_QUERY_ITEMS;
use solana_rpc_client_api::response::{
    RpcConfirmedTransactionStatusWithSignature, RpcVersionInfo, RpcVoteAccountStatus,
};
use solana_rpc_client_api::{
    config::{
        RpcAccountInfoConfig, RpcContextConfig, RpcProgramAccountsConfig, RpcSendTransactionConfig,
        RpcSimulateTransactionConfig, RpcTokenAccountsFilter,
    },
    filter::{Memcmp, RpcFilterType},
    request::{TokenAccountsFilter, MAX_GET_PROGRAM_ACCOUNT_FILTERS, MAX_MULTIPLE_ACCOUNTS},
    response::{
        OptionalContext, Response as RpcResponse, RpcBlockhash, RpcKeyedAccount,
        RpcResponseContext, RpcSimulateTransactionResult,
    },
};
use solana_runtime_api::error::Error;
use solana_sdk::clock::Slot;
use solana_sdk::signature::Signature;
use solana_sdk::{
    account::{Account, ReadableAccount},
    clock::UnixTimestamp,
    commitment_config::{CommitmentConfig, CommitmentLevel},
    inner_instruction::InnerInstructions,
    message::{v0::LoadedAddresses, AccountKeys, VersionedMessage},
    packet::PACKET_DATA_SIZE,
    program_pack::Pack,
    pubkey::{Pubkey, PUBKEY_BYTES},
    transaction::{Result as TransactionResult, VersionedTransaction},
    transaction_context::{TransactionAccount, TransactionReturnData},
};
use solana_transaction_status::{
    map_inner_instructions, parse_ui_inner_instructions, TransactionBinaryEncoding,
    TransactionStatus, UiTransactionEncoding,
};
use spl_token_2022::{
    extension::{
        interest_bearing_mint::InterestBearingConfig, BaseStateWithExtensions, StateWithExtensions,
    },
    state::{Account as TokenAccount, Mint},
};
use std::{any::type_name, cmp::min, collections::HashMap, str::FromStr, sync::Arc};

// twox128("Timestamp") + twox128("Now")
const TIMESTAMP_KEY: &str = "0xf0c365c3cf59d671eb72da0e7a4113c49f1f0515f462cdcf84e0f1d6045dfcbb";
// twox128("Solana") + twox128("Slot")
const SLOT_KEY: &str = "0xe4ad0d288d0ccf2a73473d025d07cea4ec862ddb18bc3dcede937c1cc93b0aae";

pub type TransactionLogMessages = Vec<String>;

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct TransactionSimulationResult {
    pub result: TransactionResult<()>,
    pub logs: TransactionLogMessages,
    pub post_simulation_accounts: Vec<TransactionAccount>,
    pub units_consumed: u64,
    pub return_data: Option<TransactionReturnData>,
    pub inner_instructions: Option<Vec<InnerInstructions>>,
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

    // #[method(name = "getInflationReward")]
    // async fn get_inflation_reward(
    //     &self,
    //     address_strs: Vec<String>,
    //     config: Option<RpcEpochConfig>,
    // ) -> RpcResult<Vec<Option<RpcInflationReward>>>;

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

    // #[method(name = "getEpochInfo")]
    // async fn get_epoch_info(&self, config: Option<RpcContextConfig>) -> RpcResult<EpochInfo>;

    // #[method(name = "getTransactionCount")]
    // async fn get_transaction_count(&self, config: Option<RpcContextConfig>) -> RpcResult<u64>;

    #[method(name = "getVersion")]
    async fn get_version(&self) -> RpcResult<RpcVersionInfo>;

    #[method(name = "requestAirdrop")]
    async fn request_airdrop(
        &self,
        pubkey_str: String,
        lamports: u64,
        config: Option<RpcRequestAirdropConfig>,
    ) -> RpcResult<String>;

    #[method(name = "getSignaturesForAddress")]
    async fn get_signatures_for_address(
        &self,
        address: String,
        config: Option<RpcSignaturesForAddressConfig>,
    ) -> RpcResult<Vec<RpcConfirmedTransactionStatusWithSignature>>;

    #[method(name = "getVoteAccounts")]
    async fn get_vote_accounts(
        &self,
        config: Option<RpcGetVoteAccountsConfig>,
    ) -> RpcResult<RpcVoteAccountStatus>;

    #[method(name = "getSignatureStatuses")]
    async fn get_signature_statuses(
        &self,
        signature_strs: Vec<String>,
        config: Option<RpcSignatureStatusConfig>,
    ) -> RpcResult<RpcResponse<Vec<Option<TransactionStatus>>>>;
}

#[derive(Clone)]
pub struct Solana<I> {
    client: Arc<Client>,
    accounts_index: Arc<I>,
}

impl<I> Solana<I> {
    pub fn new(client: Arc<Client>, accounts_index: Arc<I>) -> Self {
        Self {
            client,
            accounts_index,
        }
    }
}

#[async_trait]
impl<I> SolanaServer for Solana<I>
where
    I: 'static + Sync + Send + traits::AccountsIndex,
{
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
            .get_hash_with_config(RpcContextConfig {
                commitment,
                min_context_slot,
            })
            .await?;
        let slot = self.get_slot(hash).await?;
        let encoding = encoding.unwrap_or(UiAccountEncoding::Binary);

        let response = self
            .get_encoded_account(&pubkey, encoding, data_slice_config, hash)
            .await?;
        Ok(RpcResponse {
            context: RpcResponseContext {
                slot,
                api_version: Default::default(),
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
            .get_hash_with_config(RpcContextConfig {
                commitment,
                min_context_slot,
            })
            .await?;
        let slot = self.get_slot(hash).await?;
        let encoding = encoding.unwrap_or(UiAccountEncoding::Binary);

        let response = self
            .get_encoded_accounts(&pubkeys, encoding, data_slice_config, hash, None)
            .await?;

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot,
                api_version: Default::default(),
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
        let program_id = verify_pubkey(&program_id_str)?;
        let (config, mut filters, with_context, sort_results) = if let Some(config) = config {
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
        let hash = self
            .get_hash_with_config(RpcContextConfig {
                commitment,
                min_context_slot,
            })
            .await?;
        let slot = self.get_slot(hash).await?;
        let encoding = encoding.unwrap_or(UiAccountEncoding::Binary);
        optimize_filters(&mut filters);

        let keyed_accounts = {
            if let Some(owner) = get_spl_token_owner_filter(&program_id, &filters) {
                self.get_filtered_spl_token_accounts_by_owner(
                    &program_id,
                    &owner,
                    filters,
                    sort_results,
                    hash,
                )
                .await?
            } else if let Some(mint) = get_spl_token_mint_filter(&program_id, &filters) {
                self.get_filtered_spl_token_accounts_by_mint(
                    &program_id,
                    &mint,
                    filters,
                    sort_results,
                    hash,
                )
                .await?
            } else {
                let indexed_keys = self
                    .get_indexed_keys(AccountIndex::ProgramId, &program_id, sort_results)
                    .await?;

                self.get_filtered_indexed_accounts(
                    &program_id,
                    indexed_keys,
                    filters,
                    sort_results,
                    hash,
                )
                .await?
            }
        };

        let accounts = if is_known_spl_token_id(&program_id)
            && encoding == UiAccountEncoding::JsonParsed
        {
            self.get_parsed_token_keyed_accounts(&keyed_accounts, hash)
                .await?
        } else {
            keyed_accounts
                .into_iter()
                .map(|(pubkey, account)| {
                    Ok(RpcKeyedAccount {
                        pubkey: pubkey.to_string(),
                        account: encode_account(&account, &pubkey, encoding, data_slice_config)?,
                    })
                })
                .collect::<RpcResult<Vec<_>>>()?
        };
        Ok(match with_context {
            true => OptionalContext::Context(RpcResponse {
                context: RpcResponseContext {
                    slot,
                    api_version: Default::default(),
                },
                value: accounts,
            }),
            false => OptionalContext::NoContext(accounts),
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

        let owner = verify_pubkey(&owner_str)?;
        let token_account_filter = verify_token_account_filter(token_account_filter)?;

        let RpcAccountInfoConfig {
            encoding,
            data_slice: data_slice_config,
            commitment,
            min_context_slot,
        } = config.unwrap_or_default();
        let hash = self
            .get_hash_with_config(RpcContextConfig {
                commitment,
                min_context_slot,
            })
            .await?;
        let slot = self.get_slot(hash).await?;
        let encoding = encoding.unwrap_or(UiAccountEncoding::Binary);
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

        let keyed_accounts = self
            .get_filtered_spl_token_accounts_by_owner(
                &token_program_id,
                &owner,
                filters,
                true,
                hash,
            )
            .await?;
        let accounts = if encoding == UiAccountEncoding::JsonParsed {
            self.get_parsed_token_keyed_accounts(&keyed_accounts, hash)
                .await?
        } else {
            keyed_accounts
                .into_iter()
                .map(|(pubkey, account)| {
                    Ok(RpcKeyedAccount {
                        pubkey: pubkey.to_string(),
                        account: encode_account(&account, &pubkey, encoding, data_slice_config)?,
                    })
                })
                .collect::<RpcResult<Vec<_>>>()?
        };

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot,
                api_version: Default::default(),
            },
            value: accounts,
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
            .get_hash_with_config(RpcContextConfig {
                commitment,
                min_context_slot,
            })
            .await?;
        let slot = self.get_slot(hash).await?;

        let Header { number, .. } = self
            .client
            .request("chain_getHeader", rpc_params!(hash))
            .await
            .map_err(|e| internal_error(Some(e.to_string())))?;

        // TODO: Complete rpc response context
        Ok(RpcResponse {
            context: RpcResponseContext {
                slot,
                api_version: Default::default(),
            },
            value: RpcBlockhash {
                blockhash: bs58::encode(hash.as_bytes()).into_string(),
                // TODO: Update with the correct value for the valid height
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

        let RpcSendTransactionConfig {
            skip_preflight: _skip_preflight,
            preflight_commitment: _preflight_commitment,
            encoding,
            max_retries: _max_retries,
            min_context_slot: _min_context_slot,
        } = config.unwrap_or_default();
        let tx_encoding = encoding.unwrap_or(UiTransactionEncoding::Base58);
        let binary_encoding = tx_encoding.into_binary_encoding().ok_or_else(|| {
            invalid_params(Some(format!(
                "unsupported encoding: {tx_encoding}. Supported encodings: base58, base64"
            )))
        })?;

        let (_wire_transaction, unsanitized_tx) =
            decode_and_deserialize::<VersionedTransaction>(data, binary_encoding)?;

        let converted_tx = self.convert_transaction(&unsanitized_tx).await?;
        let _hash = self.submit_transaction(converted_tx).await?;

        Ok(unsanitized_tx.signatures[0].to_string())
    }

    async fn simulate_transaction(
        &self,
        data: String,
        config: Option<RpcSimulateTransactionConfig>,
    ) -> RpcResult<RpcResponse<RpcSimulateTransactionResult>> {
        tracing::debug!("simulate_transaction rpc request received");
        let RpcSimulateTransactionConfig {
            sig_verify,
            replace_recent_blockhash,
            commitment,
            encoding,
            accounts: config_accounts,
            min_context_slot,
            inner_instructions: enable_cpi_recording,
        } = config.unwrap_or_default();
        let tx_encoding = encoding.unwrap_or(UiTransactionEncoding::Base58);
        let binary_encoding = tx_encoding.into_binary_encoding().ok_or_else(|| {
            invalid_params(Some(format!(
                "unsupported encoding: {tx_encoding}. Supported encodings: base58, base64"
            )))
        })?;
        let (_, mut unsanitized_tx) =
            decode_and_deserialize::<VersionedTransaction>(data, binary_encoding)?;

        let hash = self
            .get_hash_with_config(RpcContextConfig {
                commitment,
                min_context_slot,
            })
            .await?;
        let slot = self.get_slot(hash).await?;
        let mut blockhash: Option<RpcBlockhash> = None;
        if replace_recent_blockhash {
            if sig_verify {
                return Err(invalid_params(Some(
                    "sigVerify may not be used with replaceRecentBlockhash".to_string(),
                )));
            }

            let recent_blockhash = self.last_blockhash().await?;
            // TODO: Update with the correct value for the valid height
            let Header {
                number: last_valid_block_height,
                ..
            } = self
                .client
                .request("chain_getHeader", rpc_params!(hash))
                .await
                .map_err(|e| internal_error(Some(e.to_string())))?;

            unsanitized_tx
                .message
                .set_recent_blockhash(recent_blockhash.0.into());
            blockhash.replace(RpcBlockhash {
                blockhash: recent_blockhash.to_string(),
                last_valid_block_height: last_valid_block_height.into(),
            });
        }

        let method = "simulateTransaction".to_string();
        let params = serde_json::to_vec(&(unsanitized_tx, sig_verify, enable_cpi_recording))
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

        let (
            TransactionSimulationResult {
                result,
                logs,
                post_simulation_accounts,
                units_consumed,
                return_data,
                inner_instructions,
            },
            (static_keys, dynamic_keys),
        ) = serde_json::from_slice::<(
            TransactionSimulationResult,
            (Vec<Pubkey>, Option<(Vec<Pubkey>, Vec<Pubkey>)>),
        )>(&response)
        .map_err(|e| internal_error(Some(e.to_string())))?;
        let dynamic_keys =
            dynamic_keys.map(|(writable, readonly)| LoadedAddresses { writable, readonly });
        let account_keys = AccountKeys::new(&static_keys, dynamic_keys.as_ref());
        let number_of_accounts = account_keys.len();

        let accounts = if let Some(config_accounts) = config_accounts {
            let accounts_encoding = config_accounts
                .encoding
                .unwrap_or(UiAccountEncoding::Base64);

            if accounts_encoding == UiAccountEncoding::Binary
                || accounts_encoding == UiAccountEncoding::Base58
            {
                return Err(invalid_params(Some(
                    "base58 encoding not supported".to_string(),
                )));
            }

            if config_accounts.addresses.len() > number_of_accounts {
                return Err(invalid_params(Some(format!(
                    "Too many accounts provided; max {number_of_accounts}"
                ))));
            }

            if result.is_err() {
                Some(vec![None; config_accounts.addresses.len()])
            } else {
                let mut post_simulation_accounts_map = HashMap::new();
                for (pubkey, data) in post_simulation_accounts {
                    post_simulation_accounts_map.insert(pubkey, data.into());
                }

                let pubkeys = config_accounts
                    .addresses
                    .iter()
                    .map(|pubkey| verify_pubkey(pubkey))
                    .collect::<Result<Vec<Pubkey>, _>>()
                    .map_err(|e| invalid_params(Some(e.to_string())))?;

                Some(
                    self.get_encoded_accounts(
                        &pubkeys,
                        accounts_encoding,
                        None,
                        hash,
                        Some(&post_simulation_accounts_map),
                    )
                    .await?,
                )
            }
        } else {
            None
        };

        let inner_instructions = inner_instructions.map(|info| {
            map_inner_instructions(info)
                .map(|converted| parse_ui_inner_instructions(converted, &account_keys))
                .collect()
        });

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot,
                api_version: Default::default(),
            },
            value: RpcSimulateTransactionResult {
                err: result.err(),
                logs: Some(logs),
                accounts,
                units_consumed: Some(units_consumed),
                return_data: return_data.map(|return_data| return_data.into()),
                inner_instructions,
                replacement_blockhash: blockhash,
            },
        })
    }

    // async fn get_inflation_reward(
    //     &self,
    //     address_strs: Vec<String>,
    //     _config: Option<RpcEpochConfig>,
    // ) -> RpcResult<Vec<Option<RpcInflationReward>>> {
    //     tracing::debug!(
    //         "get_inflation_reward rpc request received: {:?}",
    //         address_strs.len()
    //     );

    //     Ok(address_strs
    //         .into_iter()
    //         .map(|_| None)
    //         .collect::<Vec<Option<RpcInflationReward>>>())
    // }

    async fn get_fee_for_message(
        &self,
        data: String,
        config: Option<RpcContextConfig>,
    ) -> RpcResult<RpcResponse<Option<u64>>> {
        tracing::debug!("get_fee_for_message rpc request received");
        let (_, message) =
            decode_and_deserialize::<VersionedMessage>(data, TransactionBinaryEncoding::Base64)?;
        let hash = self
            .get_hash_with_config(config.unwrap_or_default())
            .await?;
        let slot = self.get_slot(hash).await?;

        let method = "getFeeForMessage".to_string();
        let params = serde_json::to_vec(&message).map_err(|e| parse_error(Some(e.to_string())))?;

        let response = state_call::<_, Result<Vec<u8>, Error>>(
            &self.client,
            "SolanaRuntimeApi_call",
            (method, params),
            Some(hash),
        )
        .await
        .map_err(|e| internal_error(Some(e.to_string())))?
        .map_err(|e| internal_error(Some(format!("{:?}", e))))?;

        let fee: Option<u64> = serde_json::from_slice::<_>(&response)
            .map_err(|e| internal_error(Some(e.to_string())))?;
        Ok(RpcResponse {
            context: RpcResponseContext {
                slot,
                api_version: Default::default(),
            },
            value: fee,
        })
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
            .get_hash_with_config(RpcContextConfig {
                commitment,
                min_context_slot,
            })
            .await?;
        let slot = self.get_slot(hash).await?;

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
                slot,
                api_version: Default::default(),
            },
            value: balance,
        })
    }

    async fn get_genesis_hash(&self) -> RpcResult<String> {
        tracing::debug!("get_genesis_hash rpc request received");

        let hash: Hash = self
            .client
            .request("chain_getBlockHash", rpc_params!(0))
            .await
            .map_err(|e| internal_error(Some(e.to_string())))?;

        Ok(bs58::encode(hash.as_bytes()).into_string())
    }

    // async fn get_epoch_info(&self, config: Option<RpcContextConfig>) -> RpcResult<EpochInfo> {
    //     tracing::debug!("get_epoch_info rpc request received");

    //     let method = "getEpochInfo".to_string();
    //     let params = serde_json::to_vec(&config).map_err(|e| parse_error(Some(e.to_string())))?;

    //     let response = state_call::<_, Result<Vec<u8>, Error>>(
    //         &self.client,
    //         "SolanaRuntimeApi_call",
    //         (method, params),
    //         None,
    //     )
    //     .await
    //     .map_err(|e| internal_error(Some(e.to_string())))?
    //     .map_err(|e| internal_error(Some(format!("{:?}", e))))?;

    //     serde_json::from_slice::<_>(&response).map_err(|e| internal_error(Some(e.to_string())))
    // }

    // async fn get_transaction_count(&self, config: Option<RpcContextConfig>) -> RpcResult<u64> {
    //     tracing::debug!("get_transaction_count rpc request received");

    //     let RpcContextConfig {
    //         commitment,
    //         min_context_slot,
    //     } = config.unwrap_or_default();
    //     let hash = self
    //         .get_hash_with_config(RpcContextConfig {
    //             commitment,
    //             min_context_slot,
    //         })
    //         .await?;

    //     let method = "getTransactionCount".to_string();
    //     let params = serde_json::to_vec("").map_err(|e| parse_error(Some(e.to_string())))?;

    //     let response = state_call::<_, Result<Vec<u8>, Error>>(
    //         &self.client,
    //         "SolanaRuntimeApi_call",
    //         (method, params),
    //         Some(hash),
    //     )
    //     .await
    //     .map_err(|e| internal_error(Some(e.to_string())))?
    //     .map_err(|e| internal_error(Some(format!("{:?}", e))))?;

    //     serde_json::from_slice::<_>(&response).map_err(|e| internal_error(Some(e.to_string())))
    // }

    async fn get_version(&self) -> RpcResult<RpcVersionInfo> {
        tracing::debug!("get_version rpc request received");

        // TODO: Get solana_core version and feature_set from node
        Ok(RpcVersionInfo {
            solana_core: "2.0.18".to_string(),
            feature_set: Some(607245837),
        })
    }

    async fn request_airdrop(
        &self,
        pubkey_str: String,
        lamports: u64,
        config: Option<RpcRequestAirdropConfig>,
    ) -> RpcResult<String> {
        tracing::debug!("request_airdrop rpc request received");
        tracing::trace!(
            "request_airdrop id={} lamports={} config: {:?}",
            pubkey_str,
            lamports,
            &config
        );

        Err(invalid_request(Some("Unsupported method".to_string())))
    }

    async fn get_signatures_for_address(
        &self,
        address: String,
        _config: Option<RpcSignaturesForAddressConfig>,
    ) -> RpcResult<Vec<RpcConfirmedTransactionStatusWithSignature>> {
        tracing::debug!(
            "get_signatures_for_address rpc request received: {:?}",
            address
        );

        Ok(Vec::new())
    }

    async fn get_vote_accounts(
        &self,
        _config: Option<RpcGetVoteAccountsConfig>,
    ) -> RpcResult<RpcVoteAccountStatus> {
        tracing::debug!("get_vote_accounts rpc request received");

        Ok(RpcVoteAccountStatus {
            current: Default::default(),
            delinquent: Default::default(),
        })
    }

    async fn get_signature_statuses(
        &self,
        signature_strs: Vec<String>,
        _config: Option<RpcSignatureStatusConfig>,
    ) -> RpcResult<RpcResponse<Vec<Option<TransactionStatus>>>> {
        tracing::debug!(
            "get_signature_statuses rpc request received: {:?}",
            signature_strs.len()
        );
        if signature_strs.len() > MAX_GET_SIGNATURE_STATUSES_QUERY_ITEMS {
            return Err(invalid_params(Some(format!(
                "Too many inputs provided; max {MAX_GET_SIGNATURE_STATUSES_QUERY_ITEMS}"
            ))));
        }
        let mut signatures: Vec<Signature> = vec![];
        for signature_str in signature_strs {
            signatures.push(verify_signature(&signature_str)?);
        }
        let hash = self
            .get_hash_with_config(RpcContextConfig {
                commitment: Some(CommitmentConfig::processed()),
                min_context_slot: None,
            })
            .await?;
        let slot = self.get_slot(hash).await?;

        let mut statuses: Vec<Option<TransactionStatus>> = vec![];
        for signature in signatures {
            let status = self.get_transaction_status(&signature).await?;
            statuses.push(status);
        }

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot,
                api_version: Default::default(),
            },
            value: statuses,
        })
    }
}

impl<I> Solana<I>
where
    I: traits::AccountsIndex,
{
    async fn get_hash_with_config(&self, config: RpcContextConfig) -> RpcResult<Hash> {
        let RpcContextConfig {
            commitment,
            min_context_slot,
        } = config;
        if let Some(_min_context_slot) = min_context_slot {
            // TODO: Handle min_context_slot
        }
        self.hash(commitment).await
    }

    async fn hash(&self, commitment: Option<CommitmentConfig>) -> RpcResult<Hash> {
        let commitment = commitment.unwrap_or_default();
        match commitment.commitment {
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
        .map_err(|e| internal_error(Some(e.to_string())))
    }

    async fn last_blockhash(&self) -> RpcResult<Hash> {
        self.client
            .request("chain_getBlockHash", rpc_params!())
            .await
            .map_err(|e| internal_error(Some(e.to_string())))
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

    async fn get_indexed_keys(
        &self,
        index: AccountIndex,
        index_key: &Pubkey,
        sort_results: bool,
    ) -> RpcResult<Vec<Pubkey>> {
        self.accounts_index
            .get_indexed_keys(&index, index_key, sort_results)
            .await
            .map_err(|e| {
                tracing::error!("{:?}", e);
                internal_error(Some(format!(
                    "Failed to get indexed keys. index: {:?}, index_key: {}",
                    index, index_key
                )))
            })
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

    pub async fn get_encoded_accounts(
        &self,
        pubkeys: &[Pubkey],
        encoding: UiAccountEncoding,
        data_slice: Option<UiDataSliceConfig>,
        hash: Hash,
        // only used for simulation results
        overwrite_accounts: Option<&HashMap<Pubkey, Account>>,
    ) -> RpcResult<Vec<Option<UiAccount>>> {
        let accounts = self
            .get_accounts_from_overwrites_or_node(pubkeys, overwrite_accounts, hash)
            .await?;

        if encoding == UiAccountEncoding::JsonParsed {
            let (spl_tokens, non_spl_tokens) = filter_known_spl_tokens(accounts.clone());

            let spl_token_ui_accounts = self
                .get_parsed_token_accounts(&spl_tokens, hash, overwrite_accounts)
                .await?;
            let non_spl_token_ui_accounts = non_spl_tokens
                .into_iter()
                .map(|(pubkey, account)| match account {
                    Some(account) => (
                        pubkey,
                        Some(encode_ui_account(
                            &pubkey, &account, encoding, None, data_slice,
                        )),
                    ),
                    None => (pubkey, None),
                })
                .collect::<HashMap<Pubkey, Option<UiAccount>>>();

            Ok(pubkeys
                .iter()
                .map(|pubkey| {
                    spl_token_ui_accounts
                        .get(pubkey)
                        .cloned()
                        .or_else(|| non_spl_token_ui_accounts.get(pubkey).cloned().flatten())
                })
                .collect())
        } else {
            let ui_accounts = accounts
                .into_iter()
                .map(|(pubkey, account)| {
                    account.map(|account| {
                        encode_ui_account(&pubkey, &account, encoding, None, data_slice)
                    })
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
        let mut params = ArrayParams::new();
        params.insert(TIMESTAMP_KEY).unwrap();
        params.insert(hash).unwrap();

        let mut response: String = self
            .client
            .request::<Option<String>>("state_getStorage", params)
            .await
            .map_err(|e| internal_error(Some(e.to_string())))?
            .ok_or(internal_error(Some("Timestamp not exist".to_string())))?;

        if response.starts_with("0x") {
            response = response.strip_prefix("0x").map(|s| s.to_string()).unwrap();
        }
        let response = hex::decode(response).map_err(|e| internal_error(Some(e.to_string())))?;

        Ok(u64::from_le_bytes(response.try_into().unwrap()) as i64)
    }

    pub async fn get_slot(&self, hash: Hash) -> RpcResult<Slot> {
        let mut params = ArrayParams::new();
        params.insert(SLOT_KEY).unwrap();
        params.insert(hash).unwrap();

        let mut response: String = self
            .client
            .request::<Option<String>>("state_getStorage", params)
            .await
            .map_err(|e| internal_error(Some(e.to_string())))?
            .ok_or(internal_error(Some("Solana slot not exist".to_string())))?;

        if response.starts_with("0x") {
            response = response.strip_prefix("0x").map(|s| s.to_string()).unwrap();
        }
        let response = hex::decode(response).map_err(|e| internal_error(Some(e.to_string())))?;

        Ok(u64::from_le_bytes(response.try_into().unwrap()))
    }

    /// Use a set of filters to get an iterator of keyed program accounts from a bank
    async fn get_filtered_indexed_accounts(
        &self,
        program_id: &Pubkey,
        indexed_keys: Vec<Pubkey>,
        mut filters: Vec<RpcFilterType>,
        _sort_results: bool,
        hash: Hash,
    ) -> RpcResult<Vec<(Pubkey, Account)>> {
        optimize_filters(&mut filters);

        let method = "getProgramAccounts".to_string();
        let params = serde_json::to_vec(&(program_id, indexed_keys, filters))
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

        serde_json::from_slice::<Vec<(Pubkey, Account)>>(&response)
            .map_err(|e| internal_error(Some(e.to_string())))
    }

    async fn get_filtered_spl_token_accounts_by_owner(
        &self,
        program_id: &Pubkey,
        owner_key: &Pubkey,
        mut filters: Vec<RpcFilterType>,
        sort_results: bool,
        hash: Hash,
    ) -> RpcResult<Vec<(Pubkey, Account)>> {
        // The by-owner accounts index checks for Token Account state and Owner address on
        // inclusion. However, due to the current AccountsDb implementation, an account may remain
        // in storage as a zero-lamport AccountSharedData::Default() after being wiped and reinitialized in
        // later updates. We include the redundant filters here to avoid returning these accounts.
        //
        // Filter on Token Account state
        filters.push(RpcFilterType::TokenAccountState);
        // Filter on Owner address
        filters.push(RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            SPL_TOKEN_ACCOUNT_OWNER_OFFSET,
            owner_key.to_bytes().into(),
        )));

        let indexed_keys = self
            .get_indexed_keys(AccountIndex::SplTokenOwner, owner_key, sort_results)
            .await?;

        self.get_filtered_indexed_accounts(program_id, indexed_keys, filters, sort_results, hash)
            .await
    }

    async fn get_filtered_spl_token_accounts_by_mint(
        &self,
        program_id: &Pubkey,
        mint_key: &Pubkey,
        mut filters: Vec<RpcFilterType>,
        sort_results: bool,
        hash: Hash,
    ) -> RpcResult<Vec<(Pubkey, Account)>> {
        // The by-mint accounts index checks for Token Account state and Mint address on inclusion.
        // However, due to the current AccountsDb implementation, an account may remain in storage
        // as be zero-lamport AccountSharedData::Default() after being wiped and reinitialized in later
        // updates. We include the redundant filters here to avoid returning these accounts.
        //
        // Filter on Token Account state
        filters.push(RpcFilterType::TokenAccountState);
        // Filter on Mint address
        filters.push(RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            SPL_TOKEN_ACCOUNT_MINT_OFFSET,
            mint_key.to_bytes().into(),
        )));

        let indexed_keys = self
            .get_indexed_keys(AccountIndex::SplTokenMint, mint_key, sort_results)
            .await?;

        self.get_filtered_indexed_accounts(program_id, indexed_keys, filters, sort_results, hash)
            .await
    }

    pub async fn get_parsed_token_accounts(
        &self,
        keyed_accounts: &[(Pubkey, Account)],
        hash: Hash,
        overwrite_accounts: Option<&HashMap<Pubkey, Account>>,
    ) -> RpcResult<HashMap<Pubkey, UiAccount>> {
        let mint_pubkeys = get_multiple_token_account_mint(keyed_accounts);
        let mint_accounts: HashMap<Pubkey, Account> = self
            .get_accounts_from_overwrites_or_node(
                &mint_pubkeys.values().cloned().collect::<Vec<Pubkey>>(),
                overwrite_accounts,
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

        Ok(keyed_accounts
            .iter()
            .map(|(pubkey, account)| {
                let additional_data = mint_pubkeys
                    .get(pubkey)
                    .and_then(|mint_pubkey| account_additional_data.get(mint_pubkey))
                    .cloned();

                (
                    *pubkey,
                    encode_ui_account(
                        pubkey,
                        account,
                        UiAccountEncoding::JsonParsed,
                        additional_data,
                        None,
                    ),
                )
            })
            .collect())
    }

    pub async fn get_parsed_token_keyed_accounts(
        &self,
        keyed_accounts: &[(Pubkey, Account)],
        hash: Hash,
    ) -> RpcResult<Vec<RpcKeyedAccount>> {
        Ok(self
            .get_parsed_token_accounts(keyed_accounts, hash, None)
            .await?
            .into_iter()
            .filter_map(
                |(pubkey, maybe_encoded_account)| match maybe_encoded_account.data {
                    UiAccountData::Json(_) => Some(RpcKeyedAccount {
                        pubkey: pubkey.to_string(),
                        account: maybe_encoded_account,
                    }),
                    _ => None,
                },
            )
            .collect())
    }

    async fn convert_transaction(
        &self,
        unsanitized_tx: &VersionedTransaction,
    ) -> RpcResult<Vec<u8>> {
        let method = "convertTransaction".to_string();
        let params =
            serde_json::to_vec(unsanitized_tx).map_err(|e| parse_error(Some(e.to_string())))?;

        state_call::<_, Result<Vec<u8>, Error>>(
            &self.client,
            "SolanaRuntimeApi_call",
            (method, params),
            None,
        )
        .await
        .map_err(|e| internal_error(Some(e.to_string())))?
        .map_err(|e| internal_error(Some(format!("{:?}", e))))
    }

    async fn submit_transaction(&self, converted_tx: Vec<u8>) -> RpcResult<Hash> {
        let transaction = format!("0x{}", hex::encode(converted_tx));
        self.client
            .request("author_submitExtrinsic", rpc_params!(transaction))
            .await
            .map_err(|e| invalid_request(Some(e.to_string())))
    }

    async fn get_accounts_from_overwrites_or_node(
        &self,
        pubkeys: &[Pubkey],
        overwrite_accounts: Option<&HashMap<Pubkey, Account>>,
        hash: Hash,
    ) -> RpcResult<Vec<(Pubkey, Option<Account>)>> {
        let mut accounts_from_overwrite = HashMap::new();
        let mut pubkeys_not_in_overwrite = Vec::new();

        for pubkey in pubkeys.iter() {
            if let Some(account) =
                overwrite_accounts.and_then(|overwrite_accounts| overwrite_accounts.get(pubkey))
            {
                accounts_from_overwrite.insert(*pubkey, account.clone());
            } else {
                pubkeys_not_in_overwrite.push(*pubkey);
            }
        }

        let accounts_from_node: HashMap<Pubkey, Option<Account>> = self
            .get_accounts(&pubkeys_not_in_overwrite, hash)
            .await?
            .into_iter()
            .collect();

        Ok(pubkeys
            .iter()
            .map(|pubkey| {
                let account = accounts_from_overwrite
                    .get(pubkey)
                    .cloned()
                    .or_else(|| accounts_from_node.get(pubkey).cloned().flatten());
                (*pubkey, account)
            })
            .collect())
    }

    async fn get_transaction_status(
        &self,
        _signature: &Signature,
    ) -> RpcResult<Option<TransactionStatus>> {
        // TODO: Return status
        Ok(None)
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

fn verify_signature(input: &str) -> RpcResult<Signature> {
    Signature::from_str(input).map_err(|e| invalid_params(Some(format!("Invalid param: {e:?}"))))
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
                        *pubkey,
                        AccountAdditionalDataV2 {
                            spl_token_additional_data: Some(additional_data),
                        },
                    )
                })
        })
        .collect()
}

pub fn filter_known_spl_tokens(
    accounts: Vec<(Pubkey, Option<Account>)>,
) -> (Vec<(Pubkey, Account)>, Vec<(Pubkey, Option<Account>)>) {
    let (spl_tokens, non_spl_tokens): (
        Vec<(Pubkey, Option<Account>)>,
        Vec<(Pubkey, Option<Account>)>,
    ) = accounts
        .into_iter()
        .partition(|(pubkey, account)| is_known_spl_token_id(pubkey) && account.is_some());
    (
        spl_tokens
            .into_iter()
            .filter_map(|(pubkey, account)| account.map(|account| (pubkey, account)))
            .collect(),
        non_spl_tokens,
    )
}

pub fn get_multiple_token_account_mint(accounts: &[(Pubkey, Account)]) -> HashMap<Pubkey, Pubkey> {
    accounts
        .iter()
        .filter_map(|(pubkey, account)| {
            get_token_account_mint(&account.data).map(|mint_pubkey| (*pubkey, mint_pubkey))
        })
        .collect()
}

/// Analyze custom filters to determine if the result will be a subset of spl-token accounts by
/// owner.
/// NOTE: `optimize_filters()` should almost always be called before using this method because of
/// the requirement that `Memcmp::raw_bytes_as_ref().is_some()`.
fn get_spl_token_owner_filter(program_id: &Pubkey, filters: &[RpcFilterType]) -> Option<Pubkey> {
    if !is_known_spl_token_id(program_id) {
        return None;
    }
    let mut data_size_filter: Option<u64> = None;
    let mut memcmp_filter: Option<&[u8]> = None;
    let mut owner_key: Option<Pubkey> = None;
    let mut incorrect_owner_len: Option<usize> = None;
    let mut token_account_state_filter = false;
    let account_packed_len = TokenAccount::get_packed_len();
    for filter in filters {
        match filter {
            RpcFilterType::DataSize(size) => data_size_filter = Some(*size),
            RpcFilterType::Memcmp(memcmp) => {
                let offset = memcmp.offset();
                if let Some(bytes) = memcmp.raw_bytes_as_ref() {
                    if offset == account_packed_len && *program_id == token_2022::id() {
                        memcmp_filter = Some(bytes);
                    } else if offset == SPL_TOKEN_ACCOUNT_OWNER_OFFSET {
                        if bytes.len() == PUBKEY_BYTES {
                            owner_key = Pubkey::try_from(bytes).ok();
                        } else {
                            incorrect_owner_len = Some(bytes.len());
                        }
                    }
                }
            }
            RpcFilterType::TokenAccountState => token_account_state_filter = true,
        }
    }
    if data_size_filter == Some(account_packed_len as u64)
        || memcmp_filter == Some(&[ACCOUNTTYPE_ACCOUNT])
        || token_account_state_filter
    {
        if let Some(incorrect_owner_len) = incorrect_owner_len {
            tracing::info!(
                "Incorrect num bytes ({:?}) provided for spl_token_owner_filter",
                incorrect_owner_len
            );
        }
        owner_key
    } else {
        tracing::debug!("spl_token program filters do not match by-owner index requisites");
        None
    }
}

/// Analyze custom filters to determine if the result will be a subset of spl-token accounts by
/// mint.
/// NOTE: `optimize_filters()` should almost always be called before using this method because of
/// the requirement that `Memcmp::raw_bytes_as_ref().is_some()`.
fn get_spl_token_mint_filter(program_id: &Pubkey, filters: &[RpcFilterType]) -> Option<Pubkey> {
    if !is_known_spl_token_id(program_id) {
        return None;
    }
    let mut data_size_filter: Option<u64> = None;
    let mut memcmp_filter: Option<&[u8]> = None;
    let mut mint: Option<Pubkey> = None;
    let mut incorrect_mint_len: Option<usize> = None;
    let mut token_account_state_filter = false;
    let account_packed_len = TokenAccount::get_packed_len();
    for filter in filters {
        match filter {
            RpcFilterType::DataSize(size) => data_size_filter = Some(*size),
            RpcFilterType::Memcmp(memcmp) => {
                let offset = memcmp.offset();
                if let Some(bytes) = memcmp.raw_bytes_as_ref() {
                    if offset == account_packed_len && *program_id == token_2022::id() {
                        memcmp_filter = Some(bytes);
                    } else if offset == SPL_TOKEN_ACCOUNT_MINT_OFFSET {
                        if bytes.len() == PUBKEY_BYTES {
                            mint = Pubkey::try_from(bytes).ok();
                        } else {
                            incorrect_mint_len = Some(bytes.len());
                        }
                    }
                }
            }
            RpcFilterType::TokenAccountState => token_account_state_filter = true,
        }
    }
    if data_size_filter == Some(account_packed_len as u64)
        || memcmp_filter == Some(&[ACCOUNTTYPE_ACCOUNT])
        || token_account_state_filter
    {
        if let Some(incorrect_mint_len) = incorrect_mint_len {
            tracing::info!(
                "Incorrect num bytes ({:?}) provided for spl_token_mint_filter",
                incorrect_mint_len
            );
        }
        mint
    } else {
        tracing::debug!("spl_token program filters do not match by-mint index requisites");
        None
    }
}

const MAX_BASE58_SIZE: usize = 1683; // Golden, bump if PACKET_DATA_SIZE changes
const MAX_BASE64_SIZE: usize = 1644; // Golden, bump if PACKET_DATA_SIZE changes
fn decode_and_deserialize<T>(
    encoded: String,
    encoding: TransactionBinaryEncoding,
) -> RpcResult<(Vec<u8>, T)>
where
    T: serde::de::DeserializeOwned,
{
    let wire_output = match encoding {
        TransactionBinaryEncoding::Base58 => {
            // inc_new_counter_info!("rpc-base58_encoded_tx", 1);
            if encoded.len() > MAX_BASE58_SIZE {
                return Err(invalid_params(Some(format!(
                    "base58 encoded {} too large: {} bytes (max: encoded/raw {}/{})",
                    type_name::<T>(),
                    encoded.len(),
                    MAX_BASE58_SIZE,
                    PACKET_DATA_SIZE,
                ))));
            }
            bs58::decode(encoded)
                .into_vec()
                .map_err(|e| invalid_params(Some(format!("invalid base58 encoding: {e:?}"))))?
        }
        TransactionBinaryEncoding::Base64 => {
            // inc_new_counter_info!("rpc-base64_encoded_tx", 1);
            if encoded.len() > MAX_BASE64_SIZE {
                return Err(invalid_params(Some(format!(
                    "base64 encoded {} too large: {} bytes (max: encoded/raw {}/{})",
                    type_name::<T>(),
                    encoded.len(),
                    MAX_BASE64_SIZE,
                    PACKET_DATA_SIZE,
                ))));
            }
            BASE64_STANDARD
                .decode(encoded)
                .map_err(|e| invalid_params(Some(format!("invalid base64 encoding: {e:?}"))))?
        }
    };
    if wire_output.len() > PACKET_DATA_SIZE {
        return Err(invalid_params(Some(format!(
            "decoded {} too large: {} bytes (max: {} bytes)",
            type_name::<T>(),
            wire_output.len(),
            PACKET_DATA_SIZE
        ))));
    }
    bincode::options()
        .with_limit(PACKET_DATA_SIZE as u64)
        .with_fixint_encoding()
        .allow_trailing_bytes()
        .deserialize_from(&wire_output[..])
        .map_err(|err| {
            invalid_params(Some(format!(
                "failed to deserialize {}: {}",
                type_name::<T>(),
                &err.to_string()
            )))
        })
        .map(|output| (wire_output, output))
}
