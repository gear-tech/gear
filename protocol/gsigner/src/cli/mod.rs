// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! CLI module for gsigner.
//!
//! This module provides a reusable CLI interface that can be integrated into other applications.
//! It separates command definitions from execution logic, making it easy to:
//! - Embed gsigner commands in other CLIs
//! - Customize output formatting
//! - Reuse command handlers programmatically

pub mod commands;
pub mod display;
pub mod handlers;
pub mod keyring_ops;
pub mod scheme;
pub mod storage;
pub mod util;

pub use commands::*;
pub use display::*;
pub use handlers::*;
pub use scheme::*;
