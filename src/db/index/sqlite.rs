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

use super::{get_index_name, AccountsIndex, Error};
use crate::db::sqlite::Sqlite;
use solana_accounts_db::accounts_index::AccountIndex;
use solana_sdk::pubkey::Pubkey;
use std::{str::FromStr, sync::Arc};

pub struct SqliteAccountsIndex {
    db: Arc<Sqlite>,
}

impl SqliteAccountsIndex {
    pub fn create(db: Arc<Sqlite>) -> Self {
        Self { db }
    }
}

impl AccountsIndex for SqliteAccountsIndex {
    fn get_indexed_keys(
        &self,
        index: AccountIndex,
        index_key: Pubkey,
    ) -> Result<Vec<Pubkey>, Error> {
        let index_name = get_index_name(&index);
        let mut stmt = self
            .db
            .conn()
            .prepare(
                "SELECT indexed_key FROM accounts_index where index_name = ?1 AND index_key = ?2",
            )
            .map_err(Error::SqliteError)?;

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

    fn insert_index(
        &self,
        index: AccountIndex,
        index_key: Pubkey,
        indexed_key: Pubkey,
    ) -> Result<(), Error> {
        let index_name = get_index_name(&index);
        let inserted = self
            .db
            .conn()
            .execute(
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

    fn create_index(&self) -> Result<(), Error> {
        let mut stmt = self
            .db
            .conn()
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='accounts_index'")
            .map_err(Error::SqliteError)?;

        if stmt.exists([]).map_err(Error::SqliteError)? {
            return Ok(());
        }

        let _ = self
            .db
            .conn()
            .execute(
                "CREATE TABLE IF NOT EXISTS accounts_index (
                        index_name TEXT NOT NULL,
                        index_key TEXT NOT NULL,
                        indexed_key TEXT NOT NULL,
                        UNIQUE (index_name, index_key)
                    )",
                [],
            )
            .map_err(Error::SqliteError)?;

        let _ = self
            .db
            .conn()
            .execute(
                "CREATE INDEX idx_accounts_index ON accounts_index (index_name, index_key)",
                [],
            )
            .map_err(Error::SqliteError)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sqlite::{Sqlite, SqliteConfig};

    #[test]
    fn test_get_indexed_keys() {
        let db = Sqlite::open(SqliteConfig::default()).unwrap();
        let accounts_index = SqliteAccountsIndex::create(Arc::new(db));

        let result = accounts_index.create_index();
        assert!(result.is_ok());

        let index = AccountIndex::SplTokenOwner;
        let index_key = Pubkey::new_unique();
        let indexed_key = Pubkey::new_unique();

        let result =
            accounts_index.insert_index(AccountIndex::SplTokenOwner, index_key, indexed_key);
        assert!(result.is_ok());

        let indexed_keys = accounts_index.get_indexed_keys(index, index_key).unwrap();
        assert!(indexed_keys[0] == indexed_key);
    }
}
