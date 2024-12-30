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

use super::Error;
use solana_accounts_db::accounts_index::AccountIndex;
use solana_sdk::pubkey::Pubkey;

#[async_trait::async_trait]
pub trait AccountsIndex {
    async fn get_indexed_keys(
        &self,
        index: &AccountIndex,
        index_key: &Pubkey,
    ) -> Result<Vec<Pubkey>, Error>;

    async fn insert_index(
        &self,
        index: &AccountIndex,
        index_key: &Pubkey,
        indexed_key: &Pubkey,
    ) -> Result<(), Error>;

    async fn create_index(&self) -> Result<(), Error>;
}
