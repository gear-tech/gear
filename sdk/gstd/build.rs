// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// `gstd` provides a way to define a custom `oom-handler`
// (on top of `#[alloc_error_handler]`), which is still unstable.
//
// Keep the Cargo feature compatible with stable Rust, and enable the Rust
// feature only when compiling with a nightly toolchain.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(gstd_nightly)");

    if rustc_version::version_meta()
        .is_ok_and(|meta| meta.channel == rustc_version::Channel::Nightly)
    {
        println!("cargo::rustc-cfg=gstd_nightly");
    }
}
