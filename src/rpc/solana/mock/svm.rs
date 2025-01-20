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

use crate::db::index::{self, sqlite::SqliteAccountsIndex, traits::AccountsIndex};
use litesvm::types::TransactionResult;
use solana_account_decoder::parse_token::is_known_spl_token_id;
use solana_accounts_db::accounts_index::AccountIndex;
use solana_sdk::{
    account::{Account, ReadableAccount},
    program_pack::Pack,
    pubkey::Pubkey,
    signature::Signature,
    transaction::VersionedTransaction,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::{
    mpsc::{UnboundedReceiver, UnboundedSender},
    oneshot::Sender,
};
use tokio::task::JoinHandle;

#[derive(Debug)]
pub enum SvmError {
    ParseError(solana_bincode::Error),
    DecodeError(String),
    Other(String),
}

pub type SvmResult = Result<Vec<u8>, SvmError>;
pub type SvmResponse = Sender<SvmResult>;

#[derive(Debug)]
pub struct SvmRequest {
    pub method: String,
    pub params: Vec<u8>,
    pub response: Option<SvmResponse>,
}

pub struct LiteSVM {
    pub tx: UnboundedSender<SvmRequest>,
    pub accounts_index: Arc<SqliteAccountsIndex>,
}

impl LiteSVM {
    pub fn new(accounts_index: Arc<SqliteAccountsIndex>) -> (Self, UnboundedReceiver<SvmRequest>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (Self { tx, accounts_index }, rx)
    }

    pub async fn run(&self, mut svm_rx: UnboundedReceiver<SvmRequest>) -> JoinHandle<()> {
        let accounts_index = self.accounts_index.clone();
        tokio::task::spawn_blocking(move || {
            let mut svm = litesvm::LiteSVM::new();
            let accounts_index = accounts_index.clone();

            loop {
                if let Ok(SvmRequest {
                    method,
                    params,
                    response,
                }) = svm_rx.try_recv()
                {
                    let result = match method.as_str() {
                        "getAccount" => LiteSVM::get_account(&svm, params),
                        "getAccounts" => LiteSVM::get_accounts(&svm, params),
                        "requestAirdrop" => LiteSVM::request_airdrop(&mut svm, params),
                        "getBalance" => LiteSVM::get_balance(&svm, params),
                        "sendTransaction" => {
                            LiteSVM::send_transaction(&mut svm, accounts_index.clone(), params)
                        }
                        "getTransactions" => LiteSVM::get_transactions(&svm, params),
                        "latestBlockhash" => LiteSVM::latest_blockhash(&svm),
                        "simulateTransaction" => LiteSVM::simulate_transaction(&svm, params),
                        "terminateSvm" => {
                            tracing::info!("Terminating LiteSVM...");
                            break;
                        }
                        _ => Err(SvmError::Other(format!("Unknown method: {}", method))),
                    };

                    response.and_then(|response| response.send(result).ok());
                } else {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        })
    }

    fn get_account(svm: &litesvm::LiteSVM, params: Vec<u8>) -> Result<Vec<u8>, SvmError> {
        let pubkey =
            solana_bincode::deserialize::<Pubkey>(&params).map_err(SvmError::ParseError)?;
        let account = svm.get_account(&pubkey);
        tracing::debug!("get_account: pubkey={:?}, account={:?}", pubkey, account);
        let response = solana_bincode::serialize(&account).map_err(SvmError::ParseError)?;

        Ok(response)
    }

    fn get_accounts(svm: &litesvm::LiteSVM, params: Vec<u8>) -> Result<Vec<u8>, SvmError> {
        let pubkeys =
            solana_bincode::deserialize::<Vec<Pubkey>>(&params).map_err(SvmError::ParseError)?;
        let accounts: Vec<(Pubkey, Option<Account>)> = pubkeys
            .iter()
            .map(|pubkey| (*pubkey, svm.get_account(pubkey)))
            .collect();
        tracing::debug!(
            "get_accounts: pubkeys={:?}, accounts={:?}",
            pubkeys,
            accounts
        );
        let response = solana_bincode::serialize(&accounts).map_err(SvmError::ParseError)?;

        Ok(response)
    }

    fn request_airdrop(svm: &mut litesvm::LiteSVM, params: Vec<u8>) -> Result<Vec<u8>, SvmError> {
        let (pubkey, lamports) =
            solana_bincode::deserialize::<(Pubkey, u64)>(&params).map_err(SvmError::ParseError)?;
        let result = svm.airdrop(&pubkey, lamports);
        tracing::debug!(
            "request_airdrop: pubkey={:?}, lamports={:?}, result={:?}",
            pubkey,
            lamports,
            result
        );
        let response = solana_bincode::serialize(&result).map_err(SvmError::ParseError)?;

        Ok(response)
    }

    fn get_balance(svm: &litesvm::LiteSVM, params: Vec<u8>) -> Result<Vec<u8>, SvmError> {
        let pubkey =
            solana_bincode::deserialize::<Pubkey>(&params).map_err(SvmError::ParseError)?;
        let balance = svm.get_balance(&pubkey);
        tracing::debug!("get_balance: pubkey={:?}, balance={:?}", pubkey, balance);
        let response = solana_bincode::serialize(&balance).map_err(SvmError::ParseError)?;

        Ok(response)
    }

    fn send_transaction(
        svm: &mut litesvm::LiteSVM,
        accounts_index: Arc<SqliteAccountsIndex>,
        params: Vec<u8>,
    ) -> Result<Vec<u8>, SvmError> {
        let unsanitized_tx = solana_bincode::deserialize::<VersionedTransaction>(&params)
            .map_err(SvmError::ParseError)?;

        if let Ok(simulate_result) = svm.simulate_transaction(unsanitized_tx.clone()) {
            tracing::debug!(
                "simulate_transaction: simulate_result={:?}",
                simulate_result
            );

            for (account_key, account) in simulate_result.post_accounts.into_iter() {
                if is_known_spl_token_id(account.owner()) {
                    if let Ok(token_account) = spl_token::state::Account::unpack(account.data()) {
                        let accounts_index = accounts_index.clone();
                        tokio::task::spawn_blocking(async move || {
                            if let Err(e) = Self::update_token_index(
                                &accounts_index,
                                account.owner(),
                                &token_account,
                                &account_key,
                            )
                            .await
                            {
                                tracing::error!("{:?}", e);
                            }
                        });
                    }
                }
            }
        }

        let result = svm.send_transaction(unsanitized_tx.clone());

        tracing::debug!(
            "send_transaction: unsanitized_tx={:?}, result={:?}",
            unsanitized_tx,
            result
        );
        let response = solana_bincode::serialize(&result).map_err(SvmError::ParseError)?;

        Ok(response)
    }

    async fn update_token_index(
        indexer: &Arc<SqliteAccountsIndex>,
        program_id: &Pubkey,
        token_account: &spl_token::state::Account,
        account_key: &Pubkey,
    ) -> Result<(), index::Error> {
        let owner_key = token_account.owner;
        let mint_key = token_account.mint;

        indexer
            .insert_index(&AccountIndex::ProgramId, program_id, account_key)
            .await?;
        indexer
            .insert_index(&AccountIndex::SplTokenOwner, &owner_key, account_key)
            .await?;
        indexer
            .insert_index(&AccountIndex::SplTokenMint, &mint_key, account_key)
            .await?;

        Ok(())
    }

    fn get_transactions(svm: &litesvm::LiteSVM, params: Vec<u8>) -> Result<Vec<u8>, SvmError> {
        let signatures =
            solana_bincode::deserialize::<Vec<Signature>>(&params).map_err(SvmError::ParseError)?;

        let results: Vec<Option<TransactionResult>> = signatures
            .iter()
            .map(|signature| svm.get_transaction(signature).cloned())
            .collect();
        tracing::debug!(
            "get_transactions: signatures={:?}, results={:?}",
            signatures,
            results
        );
        let response = solana_bincode::serialize(&results).map_err(SvmError::ParseError)?;

        Ok(response)
    }

    fn latest_blockhash(svm: &litesvm::LiteSVM) -> Result<Vec<u8>, SvmError> {
        let hash = svm.latest_blockhash();

        tracing::debug!("latest_blockhash: hash={:?}", hash);
        let response = solana_bincode::serialize(&hash).map_err(SvmError::ParseError)?;

        Ok(response)
    }

    fn simulate_transaction(svm: &litesvm::LiteSVM, params: Vec<u8>) -> Result<Vec<u8>, SvmError> {
        let unsanitized_tx = solana_bincode::deserialize::<VersionedTransaction>(&params)
            .map_err(SvmError::ParseError)?;

        let result = svm.simulate_transaction(unsanitized_tx.clone());
        tracing::debug!(
            "simulate_transaction: unsanitized_tx={:?}, results={:?}",
            unsanitized_tx,
            result
        );
        let response = solana_bincode::serialize(&result).map_err(SvmError::ParseError)?;

        Ok(response)
    }
}
