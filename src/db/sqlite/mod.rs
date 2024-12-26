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

pub mod query;

use super::Error;
use rusqlite::Connection;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize)]
pub struct SqliteConfig {
    path: Option<String>,
}

#[derive(Clone)]
pub struct Sqlite {
    db: Arc<Connection>,
}

impl Sqlite {
    pub fn create(config: SqliteConfig) -> Result<Self, Error> {
        let db = match config.path {
            Some(path) => Connection::open(path),
            None => Connection::open_in_memory(),
        }
        .map(Arc::new)
        .map_err(|_| Error::ConnectFailed)?;

        Ok(Self { db })
    }
}
