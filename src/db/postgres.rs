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

use r2d2_postgres::{
    postgres::{Config, NoTls},
    r2d2::{Pool, PooledConnection},
    PostgresConnectionManager,
};
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PostgresConfig {
    host: Option<String>,
    port: Option<u16>,
    dbname: Option<String>,
    user: Option<String>,
    password: Option<String>,
}

pub struct Postgres {
    pool: Pool<PostgresConnectionManager<NoTls>>,
}

impl Postgres {
    pub fn create_pool(config: PostgresConfig) -> Self {
        let postgres_config = Config::new()
            .host(&config.host.unwrap_or("127.0.0.1".to_string()))
            .port(config.port.unwrap_or(5432))
            .dbname(&config.dbname.unwrap_or("postgres".to_string()))
            .user(&config.user.unwrap_or("postgres".to_string()))
            .password(config.password.unwrap_or("postgres".to_string()))
            .to_owned();
        let manager = PostgresConnectionManager::new(postgres_config, NoTls);
        let pool = Pool::new(manager).expect("Failed to create pool");

        Self { pool }
    }

    pub fn conn(&self) -> Option<PooledConnection<PostgresConnectionManager<NoTls>>> {
        self.pool.get().ok()
    }
}
