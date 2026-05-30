// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

mod block;
mod code;
mod dev;
mod injected;
mod program;

#[cfg(feature = "client")]
pub use crate::apis::{
    block::BlockClient,
    code::CodeClient,
    dev::DevClient,
    injected::InjectedClient,
    program::{CalculateReplyForHandleResult, FullProgramState, ProgramClient},
};
#[cfg(feature = "server")]
pub use block::{BlockApi, BlockServer};
#[cfg(feature = "server")]
pub use code::{CodeApi, CodeServer};
#[cfg(feature = "server")]
pub use dev::{DevApi, DevServer};
#[cfg(feature = "server")]
pub use injected::{InjectedApi, InjectedServer};
#[cfg(feature = "server")]
pub use program::{ProgramApi, ProgramServer};
