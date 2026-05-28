// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Functions and helpers for creating programs from programs.
//!
//! Any program being an actor, can not only process incoming messages and send
//! outcoming messages to other actors but also create new actors. This feature
//! can be useful when implementing the factory pattern, as a single
//! actor can produce multiple derived actors with different input data.
//!
//! Firstly you need to upload a Wasm code of the future program(s) by calling
//! `gear.uploadCode` extrinsic to obtain the corresponding
//! [`CodeId`](crate::CodeId).
//!
//! You must also provide a unique byte sequence to create multiple program
//! instances from the same code. This sequence is often referenced as _salt_.
//! [`ProgramGenerator`] allows generating of salt automatically.
//!
//! The newly created program should be initialized using a corresponding
//! payload; therefore, you must provide it when calling any `create_program_*`
//! function.

mod generator;
pub use generator::ProgramGenerator;

mod basic;
pub use basic::*;

mod encoded;
pub use encoded::*;
