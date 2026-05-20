// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Cargo extension for building gear programs.

#![deny(missing_docs)]
#![allow(unused)]

mod artifact;
mod cli;
mod command;
mod metadata;
mod utils;

pub use self::{artifact::Artifact, cli::GBuild, command::Command};
