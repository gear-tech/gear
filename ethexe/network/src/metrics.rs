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

use libp2p::metrics::{BandwidthTransport, Metrics, Recorder, Registry};
use std::fmt::Write;

pub struct Libp2pMetrics {
    registry: Registry,
    metrics: Metrics,
}

impl Libp2pMetrics {
    pub fn new() -> Libp2pMetrics {
        let mut registry = Registry::default();
        let metrics = Metrics::new(&mut registry);
        Self { registry, metrics }
    }

    pub fn create_bandwidth_transport<T>(&mut self, transport: T) -> BandwidthTransport<T> {
        BandwidthTransport::new(transport, &mut self.registry)
    }

    pub fn render(&self, writer: &mut impl Write) {
        prometheus_client::encoding::text::encode_registry(writer, &self.registry)
            .expect("failed to encode metrics");
    }
}

impl<E> Recorder<E> for Libp2pMetrics
where
    Metrics: Recorder<E>,
{
    fn record(&self, event: &E) {
        self.metrics.record(event);
    }
}
