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
use crate::db::sqlite::{Sqlite, SqliteConfig};
use solana_accounts_db::accounts_index::AccountIndex;
use solana_sdk::pubkey::Pubkey;
use std::{str::FromStr, sync::Arc};

pub struct SqliteAccountsIndex {
    db: Arc<Sqlite>,
}

impl SqliteAccountsIndex {
    pub fn create(config: Option<SqliteConfig>) -> Result<Self, anyhow::Error> {
        Ok(Self {
            db: Arc::new(Sqlite::open(config)?),
        })
    }
}

#[async_trait::async_trait]
impl traits::AccountsIndex for SqliteAccountsIndex {
    async fn get_indexed_keys(
        &self,
        index: &AccountIndex,
        index_key: &Pubkey,
        sort_results: bool,
    ) -> Result<Vec<Pubkey>, Error> {
        let index_name = get_index_name(index);
        let conn = self
            .db
            .conn
            .as_ref()
            .lock()
            .map_err(|_| Error::MutexError)?;

        let sql = if sort_results {
            "SELECT indexed_key FROM accounts_index where index_name = ?1 AND index_key = ?2 ORDER BY indexed_key"
        } else {
            "SELECT indexed_key FROM accounts_index where index_name = ?1 AND index_key = ?2"
        };

        let mut stmt = conn.prepare(sql).map_err(Error::SqliteError)?;

        let rows = stmt
            .query_map([index_name, &index_key.to_string()], |row| {
                row.get::<_, String>(0)
            })
            .map_err(Error::SqliteError)?;

        let mut pubkeys = Vec::new();
        for row in rows {
            let key = row.map_err(Error::SqliteError)?;
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
        let conn = self
            .db
            .conn
            .as_ref()
            .lock()
            .map_err(|_| Error::MutexError)?;

        let inserted = conn.execute(
                "INSERT INTO accounts_index (index_name, index_key, indexed_key) VALUES (?1, ?2, ?3)",
                [index_name, &index_key.to_string(), &indexed_key.to_string()],
            )
            .map_err(Error::SqliteError)?;
        if inserted == 0 {
            return Err(Error::SqliteError(rusqlite::Error::StatementChangedRows(
                inserted,
            )));
        }
        Ok(())
    }

    async fn create_index(&self) -> Result<(), Error> {
        let conn = self
            .db
            .conn
            .as_ref()
            .lock()
            .map_err(|_| Error::MutexError)?;

        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='accounts_index'")
            .map_err(Error::SqliteError)?;

        if stmt.exists([]).map_err(Error::SqliteError)? {
            return Ok(());
        }

        let _ = conn
            .execute_batch(
                "BEGIN;
                CREATE TABLE IF NOT EXISTS accounts_index (
                    index_name TEXT NOT NULL,
                    index_key TEXT NOT NULL,
                    indexed_key TEXT NOT NULL,
                    UNIQUE (index_name, index_key, indexed_key)
                );
                CREATE INDEX index_accounts_index_index_name_and_index_key ON accounts_index (index_name, index_key);
                CREATE INDEX index_accounts_index_indexed_key ON accounts_index (indexed_key);
                COMMIT;",
            )
            .map_err(Error::SqliteError)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sqlite::SqliteConfig;
    use traits::AccountsIndex;

    #[tokio::test]
    async fn test_get_indexed_keys() {
        let accounts_index = SqliteAccountsIndex::create(Some(SqliteConfig::default())).unwrap();

        let result = accounts_index.create_index().await;
        assert!(result.is_ok());

        let index = AccountIndex::SplTokenOwner;
        let index_key = Pubkey::new_unique();
        let indexed_key = Pubkey::new_unique();

        let result = accounts_index
            .insert_index(&AccountIndex::SplTokenOwner, &index_key, &indexed_key)
            .await;
        assert!(result.is_ok());

        let indexed_keys = accounts_index
            .get_indexed_keys(&index, &index_key, false)
            .await
            .unwrap();
        assert!(indexed_keys[0] == indexed_key);
    }
}
