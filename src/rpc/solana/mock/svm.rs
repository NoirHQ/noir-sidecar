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

use litesvm::types::TransactionResult;
use solana_sdk::{
    account::Account, pubkey::Pubkey, signature::Signature, transaction::VersionedTransaction,
};
use std::time::Duration;
use tokio::sync::{mpsc::UnboundedReceiver, oneshot::Sender};
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

pub struct LiteSVM;

impl LiteSVM {
    pub async fn run(mut svm_rx: UnboundedReceiver<SvmRequest>) -> JoinHandle<()> {
        tokio::task::spawn_blocking(move || {
            let mut svm = litesvm::LiteSVM::new();

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
                        "sendTransaction" => LiteSVM::send_transaction(&mut svm, params),
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

    fn send_transaction(svm: &mut litesvm::LiteSVM, params: Vec<u8>) -> Result<Vec<u8>, SvmError> {
        let unsanitized_tx = solana_bincode::deserialize::<VersionedTransaction>(&params)
            .map_err(SvmError::ParseError)?;

        let result = svm.send_transaction(unsanitized_tx.clone());
        tracing::debug!(
            "send_transaction: unsanitized_tx={:?}, result={:?}",
            unsanitized_tx,
            result
        );
        let response = solana_bincode::serialize(&result).map_err(SvmError::ParseError)?;

        Ok(response)
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
