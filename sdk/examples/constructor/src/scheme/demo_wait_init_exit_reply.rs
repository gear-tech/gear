// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{Calls, Scheme};

pub fn init() -> Calls {
    let source_var = "source_var";

    Calls::builder()
        // Storing source id under `source_var`.
        .source(source_var)
        .send(source_var, [])
        .wait()
}

pub fn handle() -> Calls {
    Calls::builder().noop()
}

pub fn handle_reply() -> Calls {
    let source_var = "source_var";

    Calls::builder()
        // Storing source id under `source_var`.
        .source(source_var)
        // Exit call.
        .exit(source_var)
}

pub fn handle_signal() -> Calls {
    Calls::builder().noop()
}

pub fn scheme() -> Scheme {
    Scheme::predefined(init(), handle(), handle_reply(), handle_signal())
}
