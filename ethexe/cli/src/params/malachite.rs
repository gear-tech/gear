// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Parameters controlling the Malachite BFT consensus service.
//!
//! Kept in its own file (mirroring [`super::network`]) because the set
//! of user-facing knobs is expected to grow considerably — peer
//! discovery, persistent peers, timeouts, gas budget, etc.

use super::MergeParams;
use anyhow::Result;
use clap::Parser;
use ethexe_malachite::MalachiteConfig;
use ethexe_service::config::MalachiteCliConfig;
use serde::Deserialize;
use std::net::SocketAddr;

/// Parameters for the Malachite consensus service.
///
/// All fields are `Option`-al so that a caller's CLI flags can override
/// a TOML file via [`MergeParams`]. Defaults are resolved in
/// [`MalachiteParams::into_config`].
#[derive(Clone, Debug, Default, Deserialize, Parser)]
#[serde(deny_unknown_fields)]
pub struct MalachiteParams {
    /// Listen address for the Malachite consensus libp2p swarm.
    ///
    /// This is a **separate** socket from `--network-listen-addr`
    /// (which serves the QUIC-based ethexe-network on port 20333 by
    /// default) — the Malachite swarm currently uses TCP and its own
    /// ed25519 peer id.
    #[arg(long, aliases = &["mala-listen-addr", "malachite-listen"])]
    #[serde(rename = "listen-addr")]
    pub malachite_listen_addr: Option<SocketAddr>,

    /// Human-readable node name reported by Malachite. Used in logs
    /// and in the libp2p `agent_version` on connection.
    #[arg(long, aliases = &["mala-moniker"])]
    #[serde(rename = "moniker")]
    pub malachite_moniker: Option<String>,
}

impl MalachiteParams {
    /// Converts CLI/TOML Malachite parameters into a service-ready
    /// [`MalachiteCliConfig`]. Missing fields fall back to sensible
    /// defaults from [`MalachiteConfig`].
    pub fn into_config(self) -> Result<MalachiteCliConfig> {
        Ok(MalachiteCliConfig {
            listen_addr: self
                .malachite_listen_addr
                .unwrap_or(MalachiteConfig::DEFAULT_LISTEN_ADDR),
            moniker: self.malachite_moniker,
        })
    }
}

impl MergeParams for MalachiteParams {
    fn merge(self, with: Self) -> Self {
        Self {
            malachite_listen_addr: self
                .malachite_listen_addr
                .or(with.malachite_listen_addr),
            malachite_moniker: self.malachite_moniker.or(with.malachite_moniker),
        }
    }
}
