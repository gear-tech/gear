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
pub use host::{GearTasksRunner, TaskSpawnerExt};

use alloc::{string::String, vec::Vec};
use parity_scale_codec::{Decode, Encode};
use sp_externalities::ExternalitiesExt;
use sp_runtime_interface::runtime_interface;

pub const TASKS_AMOUNT: usize = 4;

/// WASM host functions for managing tasks.
#[runtime_interface]
pub trait GearTasks {
    fn init(&mut self) {
        self.register_extension(host::TaskSpawnerExt::default())
            .expect("`GearTasks` initialized twice");
    }

    fn check_context(&mut self) {
        self.extension::<host::GearTasksContextExt>()
            .expect("`GearTasksApi::execute_task()` called without context");
    }

    fn spawn(&mut self, func_ref: u64, payload: Vec<u8>) -> u64 {
        let spawner = self
            .extension::<host::TaskSpawnerExt>()
            .expect("Cannot spawn without dynamic runtime dispatcher (TaskSpawnerExt)");
        let handle = spawner.spawn(func_ref, payload);
        handle.inner
    }

    fn join(&mut self, handle: u64) -> JoinResult {
        let spawner = self
            .extension::<host::TaskSpawnerExt>()
            .expect("Cannot join without dynamic runtime dispatcher (TaskSpawnerExt)");
        spawner.join(JoinHandle { inner: handle })
    }
}

pub fn runtime_api_impl(func_ref: u64, payload: Vec<u8>) -> Vec<u8> {
    // safety check that nobody calls the API outside
    gear_tasks::check_context();

    #[cfg(target_arch = "wasm32")]
    let f = unsafe { core::mem::transmute::<u32, fn(Vec<u8>) -> Vec<u8>>(func_ref as u32) };

    // used only in tests
    #[cfg(feature = "std")]
    let f = unsafe { core::mem::transmute::<u64, fn(Vec<u8>) -> Vec<u8>>(func_ref) };

    // tasks must only read storage and never write
    let output_payload;
    //frame_support::assert_storage_noop!({
    output_payload = f(payload);
    //});
    output_payload
}

#[derive(Debug, Encode, Decode)]
pub enum JoinError {
    RuntimeApi(String),
}

pub type JoinResult = Result<Vec<u8>, JoinError>;

#[derive(Debug, Eq, PartialEq)]
pub struct JoinHandle {
    pub(crate) inner: u64,
}

impl JoinHandle {
    pub fn join(self) -> JoinResult {
        gear_tasks::join(self.inner)
    }
}

pub fn spawn(f: fn(Vec<u8>) -> Vec<u8>, payload: Vec<u8>) -> JoinHandle {
    let inner = gear_tasks::spawn(f as usize as u64, payload);
    JoinHandle { inner }
}
