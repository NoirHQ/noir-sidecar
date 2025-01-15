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
use crate::client::Client;
use solana_accounts_db::accounts_index::AccountIndex;
use solana_sdk::pubkey::ParsePubkeyError;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
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
    pub tx: UnboundedSender<Vec<u8>>,
}

impl<I> AccountsIndex<I>
where
    I: traits::AccountsIndex,
{
    pub fn create(client: Arc<Client>, indexer: Arc<I>) -> (Self, UnboundedReceiver<Vec<u8>>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (
            Self {
                client,
                indexer,
                tx,
            },
            rx,
        )
    }

    pub async fn initialize(&self) -> Result<(), Error> {
        self.indexer.create_index().await
    }

    pub async fn run(&self, mut rx: UnboundedReceiver<Vec<u8>>) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                if let Some(pubkeys) = rx.recv().await {}
            }
        })
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
