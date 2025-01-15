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

use crate::client::ClientConfig;
use std::time::Duration;
use subxt::{OnlineClient, PolkadotConfig};
use tokio::{sync::mpsc::UnboundedSender, task::JoinHandle};

#[derive(Clone)]
pub struct EventFilter {
    pub pallet_name: String,
    pub event_name: String,
    pub tx: UnboundedSender<Vec<u8>>,
}

impl EventFilter {
    pub fn new(pallet_name: String, event_name: String, tx: UnboundedSender<Vec<u8>>) -> Self {
        Self {
            pallet_name,
            event_name,
            tx,
        }
    }
}

#[derive(Clone)]
pub struct EventSubscriber {
    pub endpoint: String,
    pub filters: Vec<EventFilter>,
}

impl EventSubscriber {
    pub fn new(config: ClientConfig) -> Self {
        Self {
            endpoint: config.endpoint,
            filters: Vec::new(),
        }
    }

    pub fn add_filter(&mut self, filter: EventFilter) {
        self.filters.push(filter);
    }

    pub async fn run(&self) -> JoinHandle<()> {
        let mut client: Option<OnlineClient<PolkadotConfig>> = None;
        let endpoint = self.endpoint.clone();
        let filters = self.filters.clone();

        tokio::spawn(async move {
            loop {
                if let Some(ref event_client) = client {
                    let mut blocks = match event_client.blocks().subscribe_finalized().await {
                        Ok(blocks) => blocks,
                        Err(e) => {
                            tracing::error!("{:?}", e);
                            // Try reconnecting client.
                            client = None;
                            continue;
                        }
                    };
                    while let Some(Ok(block)) = blocks.next().await {
                        let events = match block.events().await {
                            Ok(events) => events,
                            Err(e) => {
                                tracing::error!("{:?}", e);
                                continue;
                            }
                        };

                        for event in events.iter().flatten() {
                            for filter in filters.iter() {
                                if event.pallet_name() == filter.pallet_name
                                    && event.variant_name() == filter.event_name
                                {
                                    if let Err(e) = filter.tx.send(event.field_bytes().to_vec()) {
                                        tracing::error!("{:?}", e);
                                    }
                                }
                            }
                        }
                    }
                } else {
                    client = subxt::OnlineClient::<PolkadotConfig>::from_url(endpoint.clone())
                        .await
                        .ok();
                    // TODO: Extract the reconnection delay.
                    // After a delay, retry connecting.
                    tokio::time::sleep(Duration::from_millis(1000)).await;
                }
            }
        })
    }
}
