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

use super::{
    encode_account, filter_known_spl_tokens, get_additional_mint_data,
    get_multiple_additional_mint_data, get_multiple_token_account_mint, optimize_filters,
    SolanaServer,
};
use crate::rpc::{
    internal_error, invalid_params,
    solana::{
        get_spl_token_mint_filter, get_spl_token_owner_filter, verify_filter, verify_pubkey,
        verify_token_account_filter,
    },
};
use jsonrpsee::core::{async_trait, RpcResult};
use solana_account_decoder::{
    encode_ui_account,
    parse_account_data::{AccountAdditionalDataV2, SplTokenAdditionalData},
    parse_token::{get_token_account_mint, is_known_spl_token_id},
    UiAccount, UiAccountData, UiAccountEncoding, UiDataSliceConfig,
};
use solana_accounts_db::{accounts_index::AccountIndex, tiered_storage::index};
use solana_inline_spl::token::{
    GenericTokenAccount, SPL_TOKEN_ACCOUNT_MINT_OFFSET, SPL_TOKEN_ACCOUNT_OWNER_OFFSET,
};
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
use solana_sdk::{
    account::{Account, ReadableAccount},
    clock::UnixTimestamp,
    epoch_info::EpochInfo,
    pubkey::Pubkey,
};
use std::collections::HashMap;

#[derive(Default)]
pub struct MockSolana {
    accounts: HashMap<Pubkey, Account>,
    accounts_index: HashMap<(AccountIndex, Pubkey), Vec<Pubkey>>,
}

#[async_trait]
impl SolanaServer for MockSolana {
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
            ..
        } = config.unwrap_or_default();
        let encoding = encoding.unwrap_or(UiAccountEncoding::Binary);

        let response = self
            .get_encoded_account(&pubkey, encoding, data_slice_config)
            .await?;
        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: Default::default(),
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
            commitment: _commitment,
            min_context_slot: _min_context_slot,
        } = config.unwrap_or_default();
        let encoding = encoding.unwrap_or(UiAccountEncoding::Binary);

        let response = self
            .get_encoded_accounts(&pubkeys, encoding, data_slice_config, None)
            .await?;

        Ok(RpcResponse {
            context: RpcResponseContext {
                slot: Default::default(),
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
            commitment: _commitment,
            min_context_slot: _min_context_slot,
        } = config.unwrap_or_default();
        let encoding = encoding.unwrap_or(UiAccountEncoding::Binary);
        optimize_filters(&mut filters);

        let keyed_accounts = {
            if let Some(owner) = get_spl_token_owner_filter(&program_id, &filters) {
                self.get_filtered_spl_token_accounts_by_owner(
                    &program_id,
                    &owner,
                    filters,
                    sort_results,
                )
                .await?
            } else if let Some(mint) = get_spl_token_mint_filter(&program_id, &filters) {
                self.get_filtered_spl_token_accounts_by_mint(
                    &program_id,
                    &mint,
                    filters,
                    sort_results,
                )
                .await?
            } else {
                let indexed_keys = self
                    .get_indexed_keys(AccountIndex::ProgramId, &program_id, sort_results)
                    .await?;

                self.get_filtered_indexed_accounts(&program_id, indexed_keys, filters, sort_results)
                    .await?
            }
        };

        let accounts = if is_known_spl_token_id(&program_id)
            && encoding == UiAccountEncoding::JsonParsed
        {
            self.get_parsed_token_keyed_accounts(&keyed_accounts)
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
                    slot: Default::default(),
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
            commitment: _commitment,
            min_context_slot: _min_context_slot,
        } = config.unwrap_or_default();
        let encoding = encoding.unwrap_or(UiAccountEncoding::Binary);
        let (token_program_id, mint) = self
            .get_token_program_id_and_mint(token_account_filter)
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
            .get_filtered_spl_token_accounts_by_owner(&token_program_id, &owner, filters, true)
            .await?;
        let accounts = if encoding == UiAccountEncoding::JsonParsed {
            self.get_parsed_token_keyed_accounts(&keyed_accounts)
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
                slot: Default::default(),
                api_version: Default::default(),
            },
            value: accounts,
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

impl MockSolana {
    /// Analyze a passed Pubkey that may be a Token program id or Mint address to determine the program
    /// id and optional Mint
    async fn get_token_program_id_and_mint(
        &self,
        token_account_filter: TokenAccountsFilter,
    ) -> RpcResult<(Pubkey, Option<Pubkey>)> {
        match token_account_filter {
            TokenAccountsFilter::Mint(mint) => {
                let (mint_owner, _) = self.get_mint_owner_and_additional_data(&mint).await?;
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
    ) -> RpcResult<(Pubkey, SplTokenAdditionalData)> {
        if mint == &spl_token::native_mint::id() {
            Ok((
                spl_token::id(),
                SplTokenAdditionalData::with_decimals(spl_token::native_mint::DECIMALS),
            ))
        } else {
            let mint_account = self.get_account(mint).await?.ok_or(invalid_params(Some(
                "Invalid param: could not find mint".to_string(),
            )))?;
            let timestamp = self.get_timestamp().await?;
            let mint_data = get_additional_mint_data(mint_account.data(), timestamp)?;
            Ok((*mint_account.owner(), mint_data))
        }
    }

    async fn get_indexed_keys(
        &self,
        index: AccountIndex,
        index_key: &Pubkey,
        _sort_results: bool,
    ) -> RpcResult<Vec<Pubkey>> {
        if let Some(indexed_keys) = self.accounts_index.get(&(index, *index_key)) {
            Ok(indexed_keys.clone())
        } else {
            Ok(Vec::new())
        }
    }

    pub async fn get_encoded_account(
        &self,
        pubkey: &Pubkey,
        encoding: UiAccountEncoding,
        data_slice: Option<UiDataSliceConfig>,
    ) -> RpcResult<Option<UiAccount>> {
        match self.get_account(pubkey).await? {
            Some(account) => {
                let response = if is_known_spl_token_id(account.owner())
                    && encoding == UiAccountEncoding::JsonParsed
                {
                    self.get_parsed_token_account(pubkey, account).await?
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
        // only used for simulation results
        overwrite_accounts: Option<&HashMap<Pubkey, Account>>,
    ) -> RpcResult<Vec<Option<UiAccount>>> {
        let accounts = self
            .get_accounts_from_overwrites_or_node(pubkeys, overwrite_accounts)
            .await?;

        if encoding == UiAccountEncoding::JsonParsed {
            let (spl_tokens, non_spl_tokens) = filter_known_spl_tokens(accounts.clone());

            let spl_token_ui_accounts = self
                .get_parsed_token_accounts(&spl_tokens, overwrite_accounts)
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
    ) -> RpcResult<UiAccount> {
        let additional_data = if let Some(mint_pubkey) = get_token_account_mint(account.data()) {
            match self.get_account(&mint_pubkey).await? {
                Some(mint_account) => {
                    let timestamp = self.get_timestamp().await?;
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

    pub async fn get_account(&self, pubkey: &Pubkey) -> RpcResult<Option<Account>> {
        Ok(self.accounts.get(pubkey).cloned())
    }

    pub async fn get_accounts(
        &self,
        pubkeys: &[Pubkey],
    ) -> RpcResult<Vec<(Pubkey, Option<Account>)>> {
        Ok(pubkeys
            .iter()
            .map(|pubkey| (*pubkey, self.accounts.get(pubkey).cloned()))
            .collect())
    }

    pub async fn get_timestamp(&self) -> RpcResult<UnixTimestamp> {
        Ok(get_mock_hash_and_slot().1 as i64)
    }

    /// Use a set of filters to get an iterator of keyed program accounts from a bank
    async fn get_filtered_indexed_accounts(
        &self,
        program_id: &Pubkey,
        indexed_keys: Vec<Pubkey>,
        mut filters: Vec<RpcFilterType>,
        _sort_results: bool,
    ) -> RpcResult<Vec<(Pubkey, Account)>> {
        optimize_filters(&mut filters);
        let filter_closure = |account: &Account| {
            filters
                .iter()
                .all(|filter_type| filter_allows(filter_type, account))
        };

        Ok(indexed_keys
            .iter()
            .filter_map(|index_key| {
                self.accounts
                    .get(index_key)
                    .filter(|account| account.owner == *program_id && filter_closure(account))
                    .map(|account| (*index_key, account.clone()))
            })
            .collect())
    }

    async fn get_filtered_spl_token_accounts_by_owner(
        &self,
        program_id: &Pubkey,
        owner_key: &Pubkey,
        mut filters: Vec<RpcFilterType>,
        sort_results: bool,
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

        self.get_filtered_indexed_accounts(program_id, indexed_keys, filters, sort_results)
            .await
    }

    async fn get_filtered_spl_token_accounts_by_mint(
        &self,
        program_id: &Pubkey,
        mint_key: &Pubkey,
        mut filters: Vec<RpcFilterType>,
        sort_results: bool,
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

        self.get_filtered_indexed_accounts(program_id, indexed_keys, filters, sort_results)
            .await
    }

    pub async fn get_parsed_token_accounts(
        &self,
        keyed_accounts: &[(Pubkey, Account)],
        overwrite_accounts: Option<&HashMap<Pubkey, Account>>,
    ) -> RpcResult<HashMap<Pubkey, UiAccount>> {
        let mint_pubkeys = get_multiple_token_account_mint(keyed_accounts);
        let mint_accounts: HashMap<Pubkey, Account> = self
            .get_accounts_from_overwrites_or_node(
                &mint_pubkeys.values().cloned().collect::<Vec<Pubkey>>(),
                overwrite_accounts,
            )
            .await?
            .into_iter()
            .filter_map(|(mint_pubkey, mint_account)| {
                mint_account.map(|account| (mint_pubkey, account))
            })
            .collect();
        let timestamp = self.get_timestamp().await?;
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
    ) -> RpcResult<Vec<RpcKeyedAccount>> {
        Ok(self
            .get_parsed_token_accounts(keyed_accounts, None)
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

    async fn get_accounts_from_overwrites_or_node(
        &self,
        pubkeys: &[Pubkey],
        overwrite_accounts: Option<&HashMap<Pubkey, Account>>,
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
            .get_accounts(&pubkeys_not_in_overwrite)
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
}

fn get_mock_hash_and_slot() -> (String, u64) {
    let slot = chrono::Utc::now().timestamp_millis() as u64;
    let mut hash = [0u8; 32];
    hash[..8].copy_from_slice(&slot.to_le_bytes());
    (bs58::encode(hash).into_string(), slot)
}

pub fn filter_allows(filter: &RpcFilterType, account: &Account) -> bool {
    match filter {
        RpcFilterType::DataSize(size) => account.data().len() as u64 == *size,
        RpcFilterType::Memcmp(compare) => compare.bytes_match(account.data()),
        RpcFilterType::TokenAccountState => {
            solana_inline_spl::token::Account::valid_account_data(account.data())
        }
    }
}
