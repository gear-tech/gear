// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::wasm::interface;
use ethexe_common::injected::Promise;
use ethexe_runtime_common::pack_u32_to_i64;
use parity_scale_codec::Encode;

interface::declare!(
    pub(super) fn ext_publish_promise(promise_ptr_len: i64);
);

/// Encode and forward a promise to the host for publication.
pub fn publish_promise(promise: &Promise) {
    unsafe {
        // Important: the `Promise` struct contains the `ReplyInfo` which have the dynamic type.
        //            So we need to encode the promise and pass to host handler a pointer and size of encoded data.

        let encoded_promise = promise.encode();
        let promise_ptr_len =
            pack_u32_to_i64(encoded_promise.as_ptr() as _, encoded_promise.len() as _);

        sys::ext_publish_promise(promise_ptr_len);
    }
}
