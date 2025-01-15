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

#[cfg(feature = "mock")]
use noir_sidecar::rpc::solana::mock::svm::{LiteSVM, SvmRequest};
#[cfg(not(feature = "mock"))]
use noir_sidecar::{
    client::Client,
    db::index::{postgres::PostgresAccountsIndex, sqlite::SqliteAccountsIndex},
};
use noir_sidecar::{
    db::index::AccountsIndex,
    event::{EventFilter, EventSubscriber},
    rpc::JsonRpcModule,
};
use std::sync::Arc;
#[cfg(feature = "mock")]
use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    noir_sidecar::logger::enable_logger();

    let args = noir_sidecar::cli::parse_args();
    let config = noir_sidecar::config::read_config(&args.config)?;

    tracing::trace!("config: {:#?}", config);

    let server = noir_sidecar::server::SidecarServer::new(config.server);

    #[cfg(not(feature = "mock"))]
    {
        let (client, client_rx) = Client::new(config.client.clone());
        let client = Arc::new(client);
        if let Some(postgres_config) = config.postgres {
            let indexer = Arc::new(PostgresAccountsIndex::create(postgres_config));
            let (accounts_index, index_rx) = AccountsIndex::create(client.clone(), indexer.clone());
            let accounts_index = Arc::new(accounts_index);
            accounts_index
                .initialize()
                .await
                .expect("Failed to initialize AccountsIndex.");

            let mut event_subscriber = EventSubscriber::new(config.client);
            event_subscriber.add_filter(EventFilter::new(
                "Solana".to_string(),
                "LoadedAccounts".to_string(),
                accounts_index.tx.clone(),
            ));
            let module = Arc::new(JsonRpcModule::create(
                client.clone(),
                accounts_index.clone(),
            )?);

            let client_task = client.run(client_rx);
            let event_task = event_subscriber.run();
            let indexer_task = accounts_index.run(index_rx);
            let server_task = server.run(module);

            tokio::join!(client_task, indexer_task, event_task, server_task);
        } else {
            let indexer = Arc::new(SqliteAccountsIndex::create(config.sqlite).unwrap());
            let (accounts_index, index_rx) = AccountsIndex::create(client.clone(), indexer.clone());
            let accounts_index = Arc::new(accounts_index);
            accounts_index
                .initialize()
                .await
                .expect("Failed to initialize AccountsIndex.");

            let mut event_subscriber = EventSubscriber::new(config.client);
            event_subscriber.add_filter(EventFilter::new(
                "Solana".to_string(),
                "LoadedAccounts".to_string(),
                accounts_index.tx.clone(),
            ));
            let module = Arc::new(JsonRpcModule::create(
                client.clone(),
                accounts_index.clone(),
            )?);

            let client_task = client.run(client_rx);
            let event_task = event_subscriber.run();
            let indexer_task = accounts_index.run(index_rx);
            let server_task = server.run(module);

            tokio::join!(client_task, indexer_task, event_task, server_task);
        }
    }

    #[cfg(feature = "mock")]
    {
        let (svm_tx, svm_rx) = tokio::sync::mpsc::unbounded_channel::<SvmRequest>();
        let module = JsonRpcModule::create(svm_tx.clone())
            .map(Arc::new)
            .expect("Failed to create jsonrpc handler.");

        tokio::spawn(async move {
            if let Ok(()) = signal::ctrl_c().await {
                svm_tx
                    .send(SvmRequest {
                        method: "terminateSvm".to_string(),
                        params: Vec::new(),
                        response: None,
                    })
                    .unwrap();
            }
        });

        let svm_task = LiteSVM::run(svm_rx);
        let server_task = server.run(module);

        tokio::join!(svm_task, server_task);
    }

    Ok(())
}
