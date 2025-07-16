// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Utility functions.

pub use scale_info::MetaType;
use scale_info::{
    PortableRegistry, Registry,
    scale::{Encode, Output},
};

use crate::prelude::{
    Box, String, Vec,
    mem::{MaybeUninit, transmute},
};

/// An auxiliary function that reduces gas consumption during payload encoding.
pub(crate) fn with_optimized_encode<T, E: Encode>(payload: E, f: impl FnOnce(&[u8]) -> T) -> T {
    struct ExternalBufferOutput<'a> {
        buffer: &'a mut [MaybeUninit<u8>],
        offset: usize,
    }

    impl Output for ExternalBufferOutput<'_> {
        fn write(&mut self, bytes: &[u8]) {
            // SAFETY: same as
            // `MaybeUninit::write_slice(&mut self.buffer[self.offset..end_offset], bytes)`.
            // This code transmutes `bytes: &[T]` to `bytes: &[MaybeUninit<T>]`. These types
            // can be safely transmuted since they have the same layout. Then `bytes:
            // &[MaybeUninit<T>]` is written to uninitialized memory via `copy_from_slice`.
            let end_offset = self.offset + bytes.len();
            let this = unsafe { self.buffer.get_unchecked_mut(self.offset..end_offset) };
            this.copy_from_slice(unsafe {
                transmute::<&[u8], &[core::mem::MaybeUninit<u8>]>(bytes)
            });
            self.offset = end_offset;
        }
    }

    gcore::stack_buffer::with_byte_buffer(payload.encoded_size(), |buffer| {
        let mut output = ExternalBufferOutput { buffer, offset: 0 };
        payload.encode_to(&mut output);
        let ExternalBufferOutput { buffer, offset } = output;
        // SAFETY: same as `MaybeUninit::slice_assume_init_ref(&buffer[..offset])`.
        // `ExternalBufferOutput` writes data to uninitialized memory. So we can take
        // slice `&buffer[..offset]` and say that it was initialized earlier
        // because the buffer from `0` to `offset` was initialized.
        let payload = unsafe { &*(&buffer[..offset] as *const _ as *const [u8]) };
        f(payload)
    })
}

/// Generate a registry from given meta types and encode it to hex.
pub fn to_hex_registry(meta_types: Vec<MetaType>) -> String {
    let mut registry = Registry::new();
    registry.register_types(meta_types);

    let registry: PortableRegistry = registry.into();
    hex::encode(registry.encode())
}

/// Convert a given reference to a raw pointer.
pub fn to_wasm_ptr<T: AsRef<[u8]>>(bytes: T) -> *mut [i32; 2] {
    Box::into_raw(Box::new([
        bytes.as_ref().as_ptr() as _,
        bytes.as_ref().len() as _,
    ]))
}

/// Convert a given vector to a raw pointer and prevent its deallocating.
///
/// It operates similarly to [`to_wasm_ptr`] except that it consumes the input
/// and make it leak by calling [`core::mem::forget`].
pub fn to_leak_ptr(bytes: impl Into<Vec<u8>>) -> *mut [i32; 2] {
    let bytes = bytes.into();
    let ptr = Box::into_raw(Box::new([bytes.as_ptr() as _, bytes.len() as _]));
    core::mem::forget(bytes);
    ptr
}
