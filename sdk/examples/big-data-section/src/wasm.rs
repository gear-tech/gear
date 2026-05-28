// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::data_access::DataAccess;
use gstd::msg;

#[unsafe(no_mangle)]
extern "C" fn init() {
    let payload = msg::load_bytes().expect("Failed to load payload");

    let value = DataAccess::from_payload(&payload)
        .expect("Failed to decode incoming payload")
        .constant();

    msg::reply_bytes(value.to_be_bytes(), 0).expect("Failed to send reply");
}
