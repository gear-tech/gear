// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

/// Wrapper around [`Memory`] that implements [`sp_allocator::Memory`].
pub struct MemoryWrapper<'a, C>(&'a wasmtime::Memory, &'a mut C);

impl<'a, C> From<(&'a wasmtime::Memory, &'a mut C)> for MemoryWrapper<'a, C> {
    fn from((memory, caller): (&'a wasmtime::Memory, &'a mut C)) -> Self {
        Self(memory, caller)
    }
}

impl<C: wasmtime::AsContextMut> sp_allocator::Memory for MemoryWrapper<'_, C> {
	fn with_access<R>(&self, run: impl FnOnce(&[u8]) -> R) -> R {
		run(self.0.data(&self.1))
	}

	fn with_access_mut<R>(&mut self, run: impl FnOnce(&mut [u8]) -> R) -> R {
		run(self.0.data_mut(&mut self.1))
	}

	fn grow(&mut self, additional: u32) -> std::result::Result<(), ()> {
		self.0
			.grow(&mut self.1, additional as u64)
			.map_err(|e| {
				log::error!(
					"Failed to grow memory by {} pages: {}",
					additional,
					e,
				)
			})
			.map(drop)
	}

	fn pages(&self) -> u32 {
		self.0.size(&self.1) as u32
	}

	fn max_pages(&self) -> Option<u32> {
		self.0.ty(&self.1).maximum().map(|p| p as _)
	}
}
