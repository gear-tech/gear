// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

//! Metering primitives and globals

use prometheus::{
    Error as PrometheusError, Opts, Registry,
    core::{AtomicU64, GenericCounterVec, GenericGaugeVec},
};
use std::sync::LazyLock;

pub static UNBOUNDED_CHANNELS_COUNTER: LazyLock<GenericCounterVec<AtomicU64>> =
    LazyLock::new(|| {
        GenericCounterVec::new(
            Opts::new(
                "ethexe_unbounded_channel_len",
                "Items sent/received/dropped on each mpsc::unbounded instance",
            ),
            &["entity", "action"], // name of channel, send|received|dropped
        )
        .expect("Creating of statics doesn't fail. qed")
    });
pub static UNBOUNDED_CHANNELS_SIZE: LazyLock<GenericGaugeVec<AtomicU64>> = LazyLock::new(|| {
    GenericGaugeVec::new(
        Opts::new(
            "ethexe_unbounded_channel_size",
            "Size (number of messages to be processed) of each mpsc::unbounded instance",
        ),
        &["entity"], // name of channel
    )
    .expect("Creating of statics doesn't fail. qed")
});

pub static SENT_LABEL: &str = "send";
pub static RECEIVED_LABEL: &str = "received";
pub static DROPPED_LABEL: &str = "dropped";

/// Register the statics to report to registry
pub fn register_globals(registry: &Registry) -> Result<(), PrometheusError> {
    registry.register(Box::new(UNBOUNDED_CHANNELS_COUNTER.clone()))?;
    registry.register(Box::new(UNBOUNDED_CHANNELS_SIZE.clone()))?;

    Ok(())
}
