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

pub mod error;
pub mod solana;

use crate::client::error::Error;
use jsonrpsee::{
    core::{client::ClientT, params::ArrayParams},
    ws_client::{WsClient, WsClientBuilder},
};
use noir_core_primitives::Hash;
use parity_scale_codec::{Decode, Encode};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{
        mpsc::{UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    task::JoinHandle,
};

#[derive(Debug, Clone, Deserialize)]
pub struct ClientConfig {
    pub endpoint: String,
    pub request_timeout_seconds: Option<u64>,
    pub connection_timeout_seconds: Option<u64>,
    pub max_concurrent_requests: Option<usize>,
    pub max_response_size: Option<u32>,
}

pub struct Client {
    config: ClientConfig,
    pub tx: UnboundedSender<Message>,
}

pub type Response = Result<Value, Error>;

#[derive(Debug)]
pub struct Request {
    pub method: String,
    pub params: ArrayParams,
    pub response: oneshot::Sender<Response>,
    pub retry: u8,
}

#[derive(Debug)]
pub enum Message {
    Request(Request),
    TryConnect,
}

impl Client {
    pub fn new(config: ClientConfig) -> (Self, UnboundedReceiver<Message>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (Self { config, tx }, rx)
    }

    pub async fn request<R>(&self, method: &str, params: ArrayParams) -> Result<R, Error>
    where
        R: DeserializeOwned,
    {
        let (res_tx, res_rx) = oneshot::channel::<Response>();
        self.tx
            .send(Message::Request(Request {
                method: method.to_string(),
                params,
                response: res_tx,
                retry: 3,
            }))
            .map_err(|e| Error::RequestFailed(format!("{:?}", e)))?;

        let response = res_rx
            .await
            .map_err(|e| Error::RequestFailed(format!("{:?}", e)))??;
        serde_json::from_value::<R>(response).map_err(|e| Error::ParseError(format!("{:?}", e)))
    }

    async fn try_connect(config: ClientConfig) -> Result<Arc<WsClient>, Error> {
        let client = Arc::new(
            WsClientBuilder::default()
                .request_timeout(
                    config
                        .request_timeout_seconds
                        .map(Duration::from_secs)
                        .unwrap_or(Duration::from_secs(30)),
                )
                .connection_timeout(
                    config
                        .connection_timeout_seconds
                        .map(Duration::from_secs)
                        .unwrap_or(Duration::from_secs(30)),
                )
                .max_concurrent_requests(config.max_concurrent_requests.unwrap_or(2048))
                .max_response_size(config.max_response_size.unwrap_or(20 * 1024 * 1024))
                .build(config.endpoint.clone())
                .await
                .map_err(Error::ClientError)?,
        );

        Ok(client)
    }

    pub async fn run(&self, mut rx: UnboundedReceiver<Message>) -> JoinHandle<()> {
        let mut client: Option<Arc<WsClient>> = None;
        let config = self.config.clone();
        let tx = self.tx.clone();

        tokio::spawn(async move {
            loop {
                if tokio::signal::ctrl_c().await.is_ok() {
                    break;
                }

                if let Some(message) = rx.recv().await {
                    tracing::debug!("{:#?}", message);

                    match message {
                        Message::Request(Request {
                            method,
                            params,
                            response: res_tx,
                            retry,
                        }) => {
                            if retry == 0 {
                                let _ = res_tx.send(Err(Error::RequestFailed(format!(
                                    "Request failed. method={}, params={:?}",
                                    method, params
                                ))));
                            } else if let Some(client) = client.as_ref() {
                                if !client.is_connected() {
                                    let _ = tx.send(Message::TryConnect);
                                    let _ = tx.send(Message::Request(Request {
                                        method,
                                        params,
                                        response: res_tx,
                                        retry: retry.saturating_sub(1),
                                    }));
                                } else {
                                    let response = client
                                        .request::<Value, ArrayParams>(&method, params)
                                        .await
                                        .map_err(Error::ClientError);
                                    res_tx.send(response).unwrap();
                                }
                            } else {
                                let _ = tx.send(Message::TryConnect);
                                let _ = tx.send(Message::Request(Request {
                                    method,
                                    params,
                                    response: res_tx,
                                    retry: retry.saturating_sub(1),
                                }));
                            }
                        }
                        Message::TryConnect => {
                            client = Self::try_connect(config.clone()).await.ok();
                        }
                    }
                } else {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        })
    }

    pub async fn state_call<I: Encode, O: Decode>(
        &self,
        method: &str,
        data: I,
        hash: Option<Hash>,
    ) -> Result<O, Error> {
        let args = format!("0x{}", hex::encode(data.encode()));

        let mut params = ArrayParams::new();
        params.insert(method).unwrap();
        params.insert(args).unwrap();
        params.insert(hash).unwrap();

        let mut res: String = self.request::<String>("state_call", params).await?;
        if res.starts_with("0x") {
            res = res.strip_prefix("0x").map(|s| s.to_string()).unwrap();
        }
        let res = hex::decode(res).map_err(|e| Error::ParseError(format!("{:?}", e)))?;

        O::decode(&mut &res[..]).map_err(|e| Error::ParseError(format!("{:?}", e)))
    }
}
