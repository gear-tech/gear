// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

mod block;
mod code;
mod dev;
mod info;
mod injected;
mod program;
#[cfg(feature = "server")]
mod program_best_state;

#[cfg(feature = "client")]
pub use crate::apis::{
    block::BlockClient,
    code::CodeClient,
    dev::DevClient,
    info::{InfoClient, RPC_VERSION},
    injected::InjectedClient,
    program::{
        CalculateReplyForHandleResult, FullProgramState, ProgramBestState, ProgramClient, Proof,
    },
};
#[cfg(feature = "server")]
pub use block::{BlockApi, BlockServer};
#[cfg(feature = "server")]
pub use code::{CodeApi, CodeServer};
#[cfg(feature = "server")]
pub use dev::{DevApi, DevServer};
#[cfg(feature = "server")]
pub use info::{InfoApi, InfoServer};
#[cfg(feature = "server")]
pub use injected::{InjectedApi, InjectedServer};
#[cfg(feature = "server")]
pub use program::{ProgramApi, ProgramServer};
#[cfg(feature = "server")]
pub use program_best_state::BestStateManager;
