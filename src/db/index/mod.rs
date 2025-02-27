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

pub mod postgres;
pub mod sqlite;
pub mod traits;

use crate::client::{error::Error as ClientError, solana::get_accounts, Client};
use noir_solana_sdk::pubkey::Pubkey as EventPubkey;
use parity_scale_codec::Decode;
use solana_account_decoder::parse_token::is_known_spl_token_id;
use solana_accounts_db::accounts_index::AccountIndex;
use solana_sdk::{
    account::Account,
    program_error::ProgramError,
    program_pack::Pack,
    pubkey::{ParsePubkeyError, Pubkey},
};
use std::{collections::HashSet, sync::Arc};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

#[derive(Debug)]
pub enum Error {
    SqliteError(rusqlite::Error),
    PostgresError(tokio_postgres::Error),
    ParsePubkeyError(ParsePubkeyError),
    UnexpectedRowCount(usize),
    MutexError,
    PoolError,
    ClientError(ClientError),
    ProgramError(ProgramError),
}

pub fn get_index_name(index: &AccountIndex) -> &'static str {
    match index {
        AccountIndex::ProgramId => "program_id",
        AccountIndex::SplTokenMint => "spl_token_mint",
        AccountIndex::SplTokenOwner => "spl_token_owner",
    }
}

pub struct AccountsIndex<I> {
    client: Arc<Client>,
    indexer: Arc<I>,
    exclude_keys: HashSet<Pubkey>,
    pub tx: UnboundedSender<Vec<u8>>,
}

impl<I> AccountsIndex<I>
where
    I: 'static + traits::AccountsIndex + std::marker::Send + std::marker::Sync,
{
    pub fn create(
        client: Arc<Client>,
        indexer: Arc<I>,
        exclude_keys: HashSet<Pubkey>,
    ) -> (Self, UnboundedReceiver<Vec<u8>>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (
            Self {
                client,
                indexer,
                exclude_keys,
                tx,
            },
            rx,
        )
    }

    pub async fn initialize(&self) -> Result<(), Error> {
        self.indexer.create_index().await
    }

    pub async fn run(&self, mut rx: UnboundedReceiver<Vec<u8>>) -> JoinHandle<()> {
        let client = self.client.clone();
        let indexer = self.indexer.clone();
        let exclude_keys = self.exclude_keys.clone();
        tokio::spawn(async move {
            loop {
                if let Some(pubkeys) = rx.recv().await {
                    match Vec::<EventPubkey>::decode(&mut &pubkeys[..]) {
                        Ok(pubkeys) => {
                            // Filter pubkeys by exclude_keys
                            let pubkeys: Vec<Pubkey> = pubkeys
                                .into_iter()
                                .map(|pubkey| Pubkey::from(pubkey.to_bytes()))
                                .filter(|pubkey| !exclude_keys.contains(pubkey))
                                .collect();
                            if let Err(e) = Self::update_index(&client, &indexer, &pubkeys).await {
                                tracing::error!("{:?}", e);
                            }
                        }
                        Err(e) => {
                            tracing::error!("{:?}", e);
                        }
                    }
                }
            }
        })
    }

    pub async fn update_index(
        client: &Arc<Client>,
        indexer: &Arc<I>,
        pubkeys: &Vec<Pubkey>,
    ) -> Result<(), Error> {
        let accounts: Vec<(Pubkey, Account)> = get_accounts(client, pubkeys, None)
            .await
            .map_err(Error::ClientError)?
            .into_iter()
            .filter_map(|(pubkey, account)| account.map(|account| (pubkey, account)))
            .collect();

        for (pubkey, account) in accounts.into_iter() {
            if is_known_spl_token_id(&account.owner) {
                if let Ok(token_account) = spl_token::state::Account::unpack(&account.data) {
                    let owner_key = token_account.owner;
                    let mint_key = token_account.mint;
                    let program_id = account.owner;

                    indexer
                        .insert_index(&AccountIndex::ProgramId, &program_id, &pubkey)
                        .await?;
                    indexer
                        .insert_index(&AccountIndex::SplTokenOwner, &owner_key, &pubkey)
                        .await?;
                    indexer
                        .insert_index(&AccountIndex::SplTokenMint, &mint_key, &pubkey)
                        .await?;
                }
            }
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl<I> traits::AccountsIndex for AccountsIndex<I>
where
    I: 'static + std::marker::Send + std::marker::Sync + traits::AccountsIndex,
{
    async fn get_indexed_keys(
        &self,
        index: &AccountIndex,
        index_key: &Pubkey,
        sort_results: bool,
    ) -> Result<Vec<Pubkey>, Error> {
        self.indexer
            .get_indexed_keys(index, index_key, sort_results)
            .await
    }

    async fn insert_index(
        &self,
        index: &AccountIndex,
        index_key: &Pubkey,
        indexed_key: &Pubkey,
    ) -> Result<(), Error> {
        self.indexer
            .insert_index(index, index_key, indexed_key)
            .await
    }

    async fn create_index(&self) -> Result<(), Error> {
        self.indexer.create_index().await
    }
}
