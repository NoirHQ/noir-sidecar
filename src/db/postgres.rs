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

use deadpool_postgres::{Config, Object, Pool, PoolConfig, Runtime};
use serde::Deserialize;
use tokio_postgres::NoTls;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PostgresConfig {
    host: Option<String>,
    port: Option<u16>,
    dbname: Option<String>,
    user: Option<String>,
    password: Option<String>,
    pool_max_size: Option<usize>,
}

pub struct Postgres {
    pool: Pool,
}

impl Postgres {
    pub fn create_pool(config: PostgresConfig) -> Self {
        let mut postgres_config = Config::new();
        postgres_config.host = Some(config.host.unwrap_or("127.0.0.1".to_string()));
        postgres_config.port = Some(config.port.unwrap_or(5432));
        postgres_config.dbname = Some(config.dbname.unwrap_or("postgres".to_string()));
        postgres_config.user = Some(config.user.unwrap_or("postgres".to_string()));
        postgres_config.password = Some(config.password.unwrap_or("postgres".to_string()));

        if let Some(max_size) = config.pool_max_size {
            postgres_config.pool = Some(PoolConfig::new(max_size));
        }

        let pool = postgres_config
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .unwrap();

        Self { pool }
    }

    pub async fn conn(&self) -> Option<Object> {
        self.pool.get().await.ok()
    }
}
