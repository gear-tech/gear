// This file is part of Gear.
//
// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::host_state::HostState;
use wasmtime::{Memory, Table};

/// Data stored in each Wasmtime store used by the Gear runtime executor.
#[derive(Default)]
pub struct StoreData {
    pub host_state: Option<HostState>,
    pub memory: Option<Memory>,
    pub table: Option<Table>,
}

impl StoreData {
    pub fn host_state_mut(&mut self) -> Option<&mut HostState> {
        self.host_state.as_mut()
    }

    pub fn memory(&self) -> Memory {
        self.memory
            .expect("memory is initialized before runtime calls; qed")
    }
}
