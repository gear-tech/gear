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

use lazy_static::lazy_static;
use prometheus::{core::AtomicU64, Error as PrometheusError, Registry};

use prometheus::{
    core::{GenericCounterVec, GenericGaugeVec},
    Opts,
};

lazy_static! {
    pub static ref UNBOUNDED_CHANNELS_COUNTER: GenericCounterVec<AtomicU64> = GenericCounterVec::new(
        Opts::new(
            "ethexe_unbounded_channel_len",
            "Items sent/received/dropped on each mpsc::unbounded instance"
        ),
        &["entity", "action"], // name of channel, send|received|dropped
    ).expect("Creating of statics doesn't fail. qed");
    pub static ref UNBOUNDED_CHANNELS_SIZE: GenericGaugeVec<AtomicU64> = GenericGaugeVec::new(
        Opts::new(
            "ethexe_unbounded_channel_size",
            "Size (number of messages to be processed) of each mpsc::unbounded instance",
        ),
        &["entity"], // name of channel
    ).expect("Creating of statics doesn't fail. qed");
}

pub static SENT_LABEL: &str = "send";
pub static RECEIVED_LABEL: &str = "received";
pub static DROPPED_LABEL: &str = "dropped";

/// Register the statics to report to registry
pub fn register_globals(registry: &Registry) -> Result<(), PrometheusError> {
    registry.register(Box::new(UNBOUNDED_CHANNELS_COUNTER.clone()))?;
    registry.register(Box::new(UNBOUNDED_CHANNELS_SIZE.clone()))?;

    Ok(())
}
