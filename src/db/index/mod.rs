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
use solana_accounts_db::accounts_index::AccountIndex;
use solana_sdk::pubkey::ParsePubkeyError;

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
