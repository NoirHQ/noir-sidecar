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

use noir_sidecar::{
    client::Client,
    db::{
        index::{
            postgres::PostgresAccountsIndex, sqlite::SqliteAccountsIndex, traits::AccountsIndex,
        },
        postgres::Postgres,
        sqlite::Sqlite,
    },
    rpc::create_rpc_module,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    noir_sidecar::logger::enable_logger();

    let args = noir_sidecar::cli::parse_args();
    let config = noir_sidecar::config::read_config(&args.config)?;

    tracing::trace!("config: {:#?}", config);

    let (tx, rx) = tokio::sync::mpsc::channel(100);
    let client = Arc::new(Client::new(config.client, tx.clone()));

    let module = {
        if let Some(config) = config.postgres {
            let db = Postgres::create_pool(config);
            let accounts_index = Arc::new(PostgresAccountsIndex::create(Arc::new(db)));
            accounts_index.create_index().unwrap();

            create_rpc_module::<PostgresAccountsIndex>(client.clone(), accounts_index)
                .map(Arc::new)
                .expect("Failed to create jsonrpc handler.")
        } else {
            let db = Sqlite::open(config.sqlite)
                .map(Arc::new)
                .expect("Failed to open sqlite database.");
            let accounts_index = Arc::new(SqliteAccountsIndex::create(db));
            accounts_index.create_index().unwrap();

            create_rpc_module::<SqliteAccountsIndex>(client.clone(), accounts_index)
                .map(Arc::new)
                .expect("Failed to create jsonrpc handler.")
        }
    };

    let server = noir_sidecar::server::SidecarServer::new(config.server);

    tokio::task::spawn(async move {
        client.run(tx, rx).await;
    });

    server.run(module).await?;

    Ok(())
}
