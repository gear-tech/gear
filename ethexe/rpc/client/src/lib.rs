// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Vara.eth RPC client APIs.
//!
//! This crate contains the generated JSON-RPC client traits and shared
//! response types for the Vara.eth node.

mod apis;

pub use ethexe_rpc_common as types;

pub use apis::{
    BlockClient, CodeClient, DevClient, InfoClient, InjectedClient, ProgramClient, RPC_VERSION,
};
