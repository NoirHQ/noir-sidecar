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

use crate::server::ServerConfig;
use anyhow::Context;
use serde::Deserialize;
use std::fs;
use std::path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
}

// read config file specified in command line
pub fn read_config(path: impl AsRef<path::Path>) -> Result<Config, anyhow::Error> {
    let path = path.as_ref();
    let templated_config_str = fs::read_to_string(path)
        .with_context(|| format!("Unable to read config file: {}", path.display()))?;

    let config: Config = toml::from_str(&templated_config_str)
        .with_context(|| format!("Unable to parse config file: {}", path.display()))?;

    Ok(config)
}
