/*
 *   Copyright (c) 2024
 *   All rights reserved.
 */
// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "std")]
mod host;

#[cfg(feature = "std")]
pub use host::{GearTasksRunner, RuntimeSetOverlayedChanges};

use alloc::{string::String, vec::Vec};
use parity_scale_codec::{Decode, Encode};
use sp_externalities::ExternalitiesExt;
use sp_runtime_interface::runtime_interface;

/// WASM host functions for managing tasks.
#[runtime_interface]
pub trait GearTasks {
    fn reinit(&mut self, tasks: u8) {
        self.register_extension(host::TaskSpawnerExt::new(tasks))
            .expect("`GearTasks` initialized twice");
    }

    /// Check that nobody calls the API outside.
    ///
    /// Used in [`runtime_api_impl`].
    fn check_context(&mut self) {
        self.extension::<host::GearTasksContextExt>()
            .expect("`GearTasksApi::execute_task()` called without context");
    }

    fn spawn(&mut self, func_ref: u64, payload: Vec<u8>) -> u64 {
        let changes = self
            .gear_overlayed_changes()
            .expect("`GearTasks::spawn` called outside `sp_state_machine::StateMachine`");

        let spawner = self
            .extension::<host::TaskSpawnerExt>()
            .expect("Cannot spawn without dynamic runtime dispatcher (TaskSpawnerExt)");
        let handle = spawner.spawn(changes, func_ref, payload);
        handle.inner
    }

    fn join(&mut self, handle: u64) -> JoinResult {
        let spawner = self
            .extension::<host::TaskSpawnerExt>()
            .expect("Cannot join without dynamic runtime dispatcher (TaskSpawnerExt)");
        spawner.join(JoinHandle { inner: handle })
    }
}

/// Implementation of `GearTasksApi::execute_task()`.
pub fn runtime_api_impl(func_ref: u64, payload: Vec<u8>) -> Vec<u8> {
    gear_tasks::check_context();

    #[cfg(target_arch = "wasm32")]
    let f = unsafe { core::mem::transmute::<u32, fn(Vec<u8>) -> Vec<u8>>(func_ref as u32) };

    #[cfg(all(feature = "std", feature = "testing"))]
    let f = unsafe { core::mem::transmute::<u64, fn(Vec<u8>) -> Vec<u8>>(func_ref) };

    #[cfg(all(feature = "std", not(feature = "testing")))]
    let f: fn(Vec<u8>) -> Vec<u8> = {
        let _ = func_ref;
        |_payload| {
            panic!(
                "`gear-tasks` runtime API implementation have not to be used for native in production"
            )
        }
    };

    f(payload)
}

/// Error returned from [`JoinHandle::join()`].
#[derive(Debug, Encode, Decode)]
pub enum JoinError {
    /// `<RuntimeApiImpl as GearTasksApi>::execute_task()` error.
    RuntimeApi(String),
}

pub type JoinResult = Result<Vec<u8>, JoinError>;

/// Handle returned from [`spawn()`].
#[must_use]
#[derive(Debug, Eq, PartialEq)]
pub struct JoinHandle {
    pub(crate) inner: u64,
}

impl JoinHandle {
    /// Wait for task completion.
    pub fn join(self) -> JoinResult {
        gear_tasks::join(self.inner)
    }
}

/// Spawn a new task using static function and payload.
pub fn spawn(f: fn(Vec<u8>) -> Vec<u8>, payload: Vec<u8>) -> JoinHandle {
    let inner = gear_tasks::spawn(f as usize as u64, payload);
    JoinHandle { inner }
}
