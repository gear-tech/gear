// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::host::{StoreData, context};
use wasmtime::{Caller, Linker};

pub fn link(linker: &mut Linker<StoreData>) -> Result<(), wasmtime::Error> {
    linker.func_wrap("env", "ext_publish_promise", publish_promise)?;

    Ok(())
}

fn publish_promise(mut caller: Caller<'_, StoreData>, promise_ptr_len: i64) {
    if let Some(sender) = caller.data().promise_sink.clone() {
        let promise = context::memory(&mut caller).decode_by_val(promise_ptr_len);

        match sender.send(promise) {
            Ok(()) => {
                log::trace!(
                    "successfully send promise to outer service: promise_ptr_len={promise_ptr_len}"
                );
            }
            Err(err) => {
                log::trace!(
                    "`publish_promise`: failed to send promise to receiver because of error={err}"
                );
            }
        }
    }
}
