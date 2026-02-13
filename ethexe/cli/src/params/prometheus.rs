// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use super::MergeParams;
use clap::Parser;
use ethexe_prometheus::PrometheusConfig;
use serde::Deserialize;
use std::net::{Ipv4Addr, SocketAddr};

/// Parameters for prometheus metrics service to start.
#[derive(Clone, Debug, Deserialize, Parser)]
#[serde(deny_unknown_fields)]
pub struct PrometheusParams {
    /// Node name in prometheus monitoring.
    #[arg(long)]
    #[serde(rename = "name")]
    pub prometheus_name: Option<String>,

    /// Port to expose prometheus metrics.
    #[arg(long)]
    #[serde(rename = "port")]
    pub prometheus_port: Option<u16>,

    /// Flag to expose prometheus metrics on all interfaces.
    #[arg(long)]
    #[serde(default, rename = "external")]
    pub prometheus_external: bool,

    /// Flag to disable prometheus metrics.
    #[arg(long)]
    #[serde(default, rename = "no-prometheus")]
    pub no_prometheus: bool,
}

impl PrometheusParams {
    pub const DEFAULT_PROMETHEUS_NAME: &str = "DevelopmentNode";
    pub const DEFAULT_PROMETHEUS_PORT: u16 = 9635;

    pub fn into_config(self) -> Option<PrometheusConfig> {
        if self.no_prometheus {
            return None;
        }

        let name = self
            .prometheus_name
            .unwrap_or_else(|| Self::DEFAULT_PROMETHEUS_NAME.into());

        let interface = if self.prometheus_external {
            Ipv4Addr::UNSPECIFIED
        } else {
            Ipv4Addr::LOCALHOST
        };

        let addr = SocketAddr::new(
            interface.into(),
            self.prometheus_port
                .unwrap_or(Self::DEFAULT_PROMETHEUS_PORT),
        );

        Some(PrometheusConfig { name, addr })
    }
}

impl MergeParams for PrometheusParams {
    fn merge(self, with: Self) -> Self {
        Self {
            prometheus_name: self.prometheus_name.or(with.prometheus_name),
            prometheus_port: self.prometheus_port.or(with.prometheus_port),
            prometheus_external: self.prometheus_external || with.prometheus_external,
            no_prometheus: self.no_prometheus || with.no_prometheus,
        }
    }
}
