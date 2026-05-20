// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! A set of common definitions that are needed for defining execution engines.

#![warn(missing_docs)]
#![deny(unused_crate_dependencies)]

pub mod error;
pub mod runtime_blob;
pub mod util;
pub mod wasm_runtime;

pub(crate) fn is_polkavm_enabled() -> bool {
    std::env::var_os("SUBSTRATE_ENABLE_POLKAVM").is_some_and(|value| value == "1")
}
