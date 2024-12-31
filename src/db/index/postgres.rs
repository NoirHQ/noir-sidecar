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

use super::{get_index_name, traits, Error};
use crate::db::postgres::Postgres;
use solana_accounts_db::accounts_index::AccountIndex;
use solana_sdk::pubkey::Pubkey;
use std::{str::FromStr, sync::Arc};

pub struct PostgresAccountsIndex {
    db: Arc<Postgres>,
}

impl PostgresAccountsIndex {
    pub fn create(db: Arc<Postgres>) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl traits::AccountsIndex for PostgresAccountsIndex {
    async fn get_indexed_keys(
        &self,
        index: &AccountIndex,
        index_key: &Pubkey,
        sort_results: bool,
    ) -> Result<Vec<Pubkey>, Error> {
        let index_name = get_index_name(index);
        let conn = self.db.as_ref().conn().await.ok_or(Error::PoolError)?;

        let sql = if sort_results {
            "SELECT indexed_key FROM accounts_index where index_name = $1 AND index_key = $2 ORDER BY indexed_key"
        } else {
            "SELECT indexed_key FROM accounts_index where index_name = $1 AND index_key = $2"
        };

        let stmt = conn.prepare(sql).await.map_err(Error::PostgresError)?;

        let rows = conn
            .query(&stmt, &[&index_name, &index_key.to_string()])
            .await
            .map_err(Error::PostgresError)?;

        let mut pubkeys = Vec::new();
        for row in rows {
            let key: String = row.get("indexed_key");
            let pubkey = Pubkey::from_str(&key).map_err(Error::ParsePubkeyError)?;
            pubkeys.push(pubkey);
        }

        Ok(pubkeys)
    }

    async fn insert_index(
        &self,
        index: &AccountIndex,
        index_key: &Pubkey,
        indexed_key: &Pubkey,
    ) -> Result<(), Error> {
        let index_name = get_index_name(index);
        let conn = self.db.as_ref().conn().await.ok_or(Error::PoolError)?;

        let inserted = conn.execute(
                "INSERT INTO accounts_index (index_name, index_key, indexed_key) VALUES ($1, $2, $3)",
                &[&index_name, &index_key.to_string(), &indexed_key.to_string()],
            )
            .await
            .map_err(Error::PostgresError)?;
        if inserted == 0 {
            return Err(Error::UnexpectedRowCount(inserted as usize));
        }
        Ok(())
    }

    async fn create_index(&self) -> Result<(), Error> {
        let conn = self.db.as_ref().conn().await.ok_or(Error::PoolError)?;

        let stmt = conn
            .prepare("SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' AND table_name='accounts_index'")
            .await
            .map_err(Error::PostgresError)?;

        if !conn
            .query(&stmt, &[])
            .await
            .map_err(Error::PostgresError)?
            .is_empty()
        {
            return Ok(());
        }

        let _ = conn
            .execute(
                "CREATE TABLE IF NOT EXISTS accounts_index (
                        index_name TEXT NOT NULL,
                        index_key TEXT NOT NULL,
                        indexed_key TEXT NOT NULL,
                        UNIQUE (index_name, index_key, indexed_key)
                    )",
                &[],
            )
            .await
            .map_err(Error::PostgresError)?;

        let _ = conn
            .execute(
                "CREATE INDEX index_accounts_index_index_name_and_index_key ON accounts_index (index_name, index_key)",
                &[],
            )
            .await
            .map_err(Error::PostgresError)?;

        let _ = conn
            .execute(
                "CREATE INDEX index_accounts_index_indexed_key ON accounts_index (indexed_key)",
                &[],
            )
            .await
            .map_err(Error::PostgresError)?;

        Ok(())
    }
}
