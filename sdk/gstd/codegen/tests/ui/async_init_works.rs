// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

fn main() {}

#[gstd::async_init(handle_signal = custom_handle_signal)]
async fn init() {}

fn custom_handle_signal() {}
