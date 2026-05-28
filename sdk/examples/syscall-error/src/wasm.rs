// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gstd::{
    ActorId,
    errors::{CoreError, ExecutionError, ExtError},
    msg,
    prelude::*,
};

#[unsafe(no_mangle)]
extern "C" fn init() {
    let res = msg::send(ActorId::default(), "dummy", u128::MAX / 2);
    assert_eq!(
        res,
        Err(CoreError::Ext(ExtError::Execution(
            ExecutionError::NotEnoughValue
        )))
    );
}
