// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Substrate mixnet service. This implements the [Substrate Mix Network
//! Specification](https://paritytech.github.io/mixnet-spec/).

#![warn(missing_docs)]
#![forbid(unsafe_code)]
#![allow(
    clippy::borrowed_box,
    clippy::derivable_impls,
    clippy::too_many_arguments
)]

mod api;
mod config;
mod error;
mod extrinsic_queue;
mod maybe_inf_delay;
mod packet_dispatcher;
mod peer_id;
mod protocol;
mod request;
mod run;
mod sync_with_runtime;

pub use self::{
    api::{Api, ApiBackend},
    config::{Config, CoreConfig, SubstrateConfig},
    error::{Error, RemoteErr},
    protocol::{peers_set_config, protocol_name},
    run::run,
};
pub use mixnet::core::{KxSecret, PostErr, TopologyErr};
