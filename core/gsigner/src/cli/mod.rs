// This file is part of Gear.
//
// Copyright (C) 2021-2026 Gear Technologies Inc.
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
