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

use crate::rpc::solana::TransactionSimulationResult;

use super::{error::Error, Client};
use jsonrpsee::{core::params::ArrayParams, rpc_params};
use noir_core_primitives::{Hash, Header};
use solana_rpc_client_api::filter::RpcFilterType;
use solana_sdk::{
    account::Account,
    clock::{Slot, UnixTimestamp},
    message::VersionedMessage,
    pubkey::Pubkey,
    transaction::VersionedTransaction,
};
use std::sync::Arc;

// twox128("Timestamp") + twox128("Now")
const TIMESTAMP_KEY: &str = "0xf0c365c3cf59d671eb72da0e7a4113c49f1f0515f462cdcf84e0f1d6045dfcbb";
// twox128("Solana") + twox128("Slot")
const SLOT_KEY: &str = "0xe4ad0d288d0ccf2a73473d025d07cea4ec862ddb18bc3dcede937c1cc93b0aae";

pub async fn get_account(
    client: &Arc<Client>,
    pubkey: &Pubkey,
    hash: Option<Hash>,
) -> Result<Option<Account>, Error> {
    let method = "getAccountInfo".to_string();
    let params =
        solana_bincode::serialize(pubkey).map_err(|e| Error::ParseError(format!("{:?}", e)))?;

    let response = client
        .state_call::<_, Result<Vec<u8>, solana_runtime_api::error::Error>>(
            "SolanaRuntimeApi_call",
            (method, params),
            hash,
        )
        .await?
        .map_err(|e| Error::InvalidRequest(format!("{:?}", e)))?;

    solana_bincode::deserialize::<Option<Account>>(&response)
        .map_err(|e| Error::ParseError(format!("{:?}", e)))
}

pub async fn get_accounts(
    client: &Arc<Client>,
    pubkeys: &Vec<Pubkey>,
    hash: Option<Hash>,
) -> Result<Vec<(Pubkey, Option<Account>)>, Error> {
    let method = "getMultipleAccounts".to_string();
    let params =
        solana_bincode::serialize(pubkeys).map_err(|e| Error::ParseError(format!("{:?}", e)))?;

    let response = client
        .state_call::<_, Result<Vec<u8>, solana_runtime_api::error::Error>>(
            "SolanaRuntimeApi_call",
            (method, params),
            hash,
        )
        .await?
        .map_err(|e| Error::InvalidRequest(format!("{:?}", e)))?;

    let accounts = solana_bincode::deserialize::<Vec<Option<Account>>>(&response)
        .map_err(|e| Error::ParseError(format!("{:?}", e)))?;

    if pubkeys.len() != accounts.len() {
        return Err(Error::UnexpectedResponse(
            "Account count mismatch with public keys.".to_string(),
        ));
    }

    Ok(pubkeys.iter().cloned().zip(accounts).collect())
}

/// Use a set of filters to get an iterator of keyed program accounts from a bank
pub async fn get_filtered_indexed_accounts(
    client: &Arc<Client>,
    program_id: &Pubkey,
    indexed_keys: Vec<Pubkey>,
    filters: Vec<RpcFilterType>,
    hash: Option<Hash>,
) -> Result<Vec<(Pubkey, Account)>, Error> {
    let method = "getProgramAccounts".to_string();
    let params = solana_bincode::serialize(&(program_id, indexed_keys, filters))
        .map_err(|e| Error::ParseError(format!("{:?}", e)))?;

    let response = client
        .state_call::<_, Result<Vec<u8>, solana_runtime_api::error::Error>>(
            "SolanaRuntimeApi_call",
            (method, params),
            hash,
        )
        .await?
        .map_err(|e| Error::InvalidRequest(format!("{:?}", e)))?;

    solana_bincode::deserialize::<Vec<(Pubkey, Account)>>(&response)
        .map_err(|e| Error::ParseError(format!("{:?}", e)))
}

pub async fn get_balance(client: &Arc<Client>, pubkey: &Pubkey, hash: Hash) -> Result<u64, Error> {
    let method = "getBalance".to_string();
    let params =
        solana_bincode::serialize(pubkey).map_err(|e| Error::ParseError(format!("{:?}", e)))?;

    let response = client
        .state_call::<_, Result<Vec<u8>, solana_runtime_api::error::Error>>(
            "SolanaRuntimeApi_call",
            (method, params),
            Some(hash),
        )
        .await?
        .map_err(|e| Error::InvalidRequest(format!("{:?}", e)))?;

    solana_bincode::deserialize::<u64>(&response).map_err(|e| Error::ParseError(format!("{:?}", e)))
}

pub async fn get_genesis_hash(client: &Arc<Client>) -> Result<String, Error> {
    let hash: Hash = client.request("chain_getBlockHash", rpc_params!(0)).await?;

    Ok(bs58::encode(hash.as_bytes()).into_string())
}

pub async fn get_timestamp(client: &Arc<Client>, hash: Hash) -> Result<UnixTimestamp, Error> {
    let mut params = ArrayParams::new();
    params.insert(TIMESTAMP_KEY).unwrap();
    params.insert(hash).unwrap();

    let mut response: String = client
        .request::<Option<String>>("state_getStorage", params)
        .await?
        .ok_or(Error::UnexpectedResponse("Timestamp not exist".to_string()))?;

    if response.starts_with("0x") {
        response = response.strip_prefix("0x").map(|s| s.to_string()).unwrap();
    }
    let response = hex::decode(response).map_err(|e| Error::ParseError(format!("{:?}", e)))?;

    Ok(u64::from_le_bytes(response.try_into().unwrap()) as i64)
}

pub async fn get_slot(client: &Arc<Client>, hash: Hash) -> Result<Slot, Error> {
    let mut params = ArrayParams::new();
    params.insert(SLOT_KEY).unwrap();
    params.insert(hash).unwrap();

    let mut response: String = client
        .request::<Option<String>>("state_getStorage", params)
        .await?
        .ok_or(Error::UnexpectedResponse(
            "Solana slot not exist".to_string(),
        ))?;

    if response.starts_with("0x") {
        response = response.strip_prefix("0x").map(|s| s.to_string()).unwrap();
    }
    let response = hex::decode(response).map_err(|e| Error::ParseError(format!("{:?}", e)))?;

    Ok(u64::from_le_bytes(response.try_into().unwrap()))
}

pub async fn get_fee_for_message(
    client: &Arc<Client>,
    message: &VersionedMessage,
    hash: Option<Hash>,
) -> Result<u64, Error> {
    let method = "getFeeForMessage".to_string();
    let params =
        solana_bincode::serialize(message).map_err(|e| Error::ParseError(format!("{:?}", e)))?;

    let response = client
        .state_call::<_, Result<Vec<u8>, solana_runtime_api::error::Error>>(
            "SolanaRuntimeApi_call",
            (method, params),
            hash,
        )
        .await?
        .map_err(|e| Error::InvalidRequest(format!("{:?}", e)))?;

    solana_bincode::deserialize::<_>(&response).map_err(|e| Error::ParseError(format!("{:?}", e)))
}

pub async fn convert_transaction(
    client: &Arc<Client>,
    unsanitized_tx: &VersionedTransaction,
) -> Result<Vec<u8>, Error> {
    let method = "convertTransaction".to_string();
    let params = solana_bincode::serialize(unsanitized_tx)
        .map_err(|e| Error::ParseError(format!("{:?}", e)))?;

    client
        .state_call::<_, Result<Vec<u8>, solana_runtime_api::error::Error>>(
            "SolanaRuntimeApi_call",
            (method, params),
            None,
        )
        .await?
        .map_err(|e| Error::InvalidRequest(format!("{:?}", e)))
}

pub async fn submit_transaction(
    client: &Arc<Client>,
    converted_tx: Vec<u8>,
) -> Result<Hash, Error> {
    let transaction = format!("0x{}", hex::encode(converted_tx));
    client
        .request("author_submitExtrinsic", rpc_params!(transaction))
        .await
        .map_err(|e| Error::InvalidRequest(format!("{:?}", e)))
}

pub async fn simulate_transaction(
    client: &Arc<Client>,
    unsanitized_tx: &VersionedTransaction,
    sig_verify: bool,
    enable_cpi_recording: bool,
    hash: Option<Hash>,
) -> Result<
    (
        TransactionSimulationResult,
        (Vec<Pubkey>, Option<(Vec<Pubkey>, Vec<Pubkey>)>),
    ),
    Error,
> {
    let method = "simulateTransaction".to_string();
    let params = solana_bincode::serialize(&(unsanitized_tx, sig_verify, enable_cpi_recording))
        .map_err(|e| Error::ParseError(format!("{:?}", e)))?;

    let response = client
        .state_call::<_, Result<Vec<u8>, solana_runtime_api::error::Error>>(
            "SolanaRuntimeApi_call",
            (method, params),
            hash,
        )
        .await?
        .map_err(|e| Error::InvalidRequest(format!("{:?}", e)))?;

    solana_bincode::deserialize::<(
        TransactionSimulationResult,
        (Vec<Pubkey>, Option<(Vec<Pubkey>, Vec<Pubkey>)>),
    )>(&response)
    .map_err(|e| Error::ParseError(format!("{:?}", e)))
}

pub async fn latest_blockhash(client: &Arc<Client>) -> Result<Hash, Error> {
    client.request("chain_getBlockHash", rpc_params!()).await
}

pub async fn finalized_blockhash(client: &Arc<Client>) -> Result<Hash, Error> {
    client
        .request("chain_getFinalizedHead", rpc_params!())
        .await
}

pub async fn get_last_valid_block_height(client: &Arc<Client>, hash: Hash) -> Result<u64, Error> {
    let Header { number, .. } = client.request("chain_getHeader", rpc_params!(hash)).await?;

    // TODO: Get max_age
    let max_age: u32 = 20;
    Ok(number.saturating_add(max_age) as u64)
}
