// This file is part of Gear.
//
// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use sp_allocator::{AllocationStats, FreeingBumpHeapAllocator};

/// State shared by all host calls during one Wasmtime runtime invocation.
pub struct HostState {
    pub(crate) allocator: Option<FreeingBumpHeapAllocator>,
    pub(crate) panic_message: Option<String>,
}

impl HostState {
    pub(crate) fn new(allocator: FreeingBumpHeapAllocator) -> Self {
        Self {
            allocator: Some(allocator),
            panic_message: None,
        }
    }

    pub(crate) fn take_panic_message(&mut self) -> Option<String> {
        self.panic_message.take()
    }

    pub(crate) fn allocation_stats(&self) -> AllocationStats {
        self.allocator
            .as_ref()
            .expect("allocator is set outside active allocation/deallocation; qed")
            .stats()
    }
}
