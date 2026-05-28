// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! A set of common definitions that are needed for defining execution engines.

#![warn(missing_docs)]

pub mod context;
pub mod error;
pub mod sandbox;
pub mod util;

pub(crate) mod store_refcell;
