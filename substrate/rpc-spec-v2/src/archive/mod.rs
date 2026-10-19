// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Substrate archive API.
//!
//! # Note
//!
//! Methods are prefixed by `archive`.

#[cfg(test)]
mod tests;

mod archive_storage;

pub mod api;
pub mod archive;
pub mod error;

pub use api::ArchiveApiServer;
pub use archive::{Archive, ArchiveConfig};
