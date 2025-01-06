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

#[derive(Debug)]
pub enum SvmError {
    ParseError(serde_json::Error),
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
    pub async fn run(mut svm_rx: UnboundedReceiver<SvmRequest>) {
        tokio::task::spawn_blocking(move || {
            let mut svm = litesvm::LiteSVM::new();
            loop {
                if let Ok(request) = svm_rx.try_recv() {
                    let result = match request.method.as_str() {
                        "getAccount" => LiteSVM::get_account(&svm, request.params),
                        "getAccounts" => LiteSVM::get_accounts(&svm, request.params),
                        "requestAirdrop" => LiteSVM::request_airdrop(&mut svm, request.params),
                        "getBalance" => LiteSVM::get_balance(&svm, request.params),
                        "sendTransaction" => LiteSVM::send_transaction(&mut svm, request.params),
                        "getTransactions" => LiteSVM::get_transactions(&svm, request.params),
                        "terminateSvm" => {
                            tracing::info!("Terminating LiteSVM...");
                            break;
                        }
                        _ => Err(SvmError::Other(format!(
                            "Unknown method: {}",
                            request.method
                        ))),
                    };

                    request
                        .response
                        .and_then(|response| response.send(result).ok());
                } else {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        })
        .await
        .unwrap();
    }

    fn get_account(svm: &litesvm::LiteSVM, params: Vec<u8>) -> Result<Vec<u8>, SvmError> {
        let pubkey = serde_json::from_slice::<Pubkey>(&params).map_err(SvmError::ParseError)?;
        let account = svm.get_account(&pubkey);
        let response = serde_json::to_vec(&account).map_err(SvmError::ParseError)?;

        Ok(response)
    }

    fn get_accounts(svm: &litesvm::LiteSVM, params: Vec<u8>) -> Result<Vec<u8>, SvmError> {
        let pubkeys =
            serde_json::from_slice::<Vec<Pubkey>>(&params).map_err(SvmError::ParseError)?;
        let accounts: Vec<(Pubkey, Option<Account>)> = pubkeys
            .into_iter()
            .map(|pubkey| (pubkey, svm.get_account(&pubkey)))
            .collect();
        let response = serde_json::to_vec(&accounts).map_err(SvmError::ParseError)?;

        Ok(response)
    }

    fn request_airdrop(svm: &mut litesvm::LiteSVM, params: Vec<u8>) -> Result<Vec<u8>, SvmError> {
        let (pubkey, lamports) =
            serde_json::from_slice::<(Pubkey, u64)>(&params).map_err(SvmError::ParseError)?;
        let result = svm.airdrop(&pubkey, lamports);
        let response = serde_json::to_vec(&result).map_err(SvmError::ParseError)?;

        Ok(response)
    }

    fn get_balance(svm: &litesvm::LiteSVM, params: Vec<u8>) -> Result<Vec<u8>, SvmError> {
        let pubkey = serde_json::from_slice::<Pubkey>(&params).map_err(SvmError::ParseError)?;
        let balance = svm.get_balance(&pubkey);
        let response = serde_json::to_vec(&balance).map_err(SvmError::ParseError)?;

        Ok(response)
    }

    fn send_transaction(svm: &mut litesvm::LiteSVM, params: Vec<u8>) -> Result<Vec<u8>, SvmError> {
        let transaction = serde_json::from_slice::<VersionedTransaction>(&params)
            .map_err(SvmError::ParseError)?;
        let result = svm.send_transaction(transaction);
        let response = serde_json::to_vec(&result).map_err(SvmError::ParseError)?;

        Ok(response)
    }

    fn get_transactions(svm: &litesvm::LiteSVM, params: Vec<u8>) -> Result<Vec<u8>, SvmError> {
        let signatures =
            serde_json::from_slice::<Vec<Signature>>(&params).map_err(SvmError::ParseError)?;
        let results: Vec<Option<TransactionResult>> = signatures
            .into_iter()
            .map(|signature| svm.get_transaction(&signature).cloned())
            .collect();
        let response = serde_json::to_vec(&results).map_err(SvmError::ParseError)?;

        Ok(response)
    }
}
