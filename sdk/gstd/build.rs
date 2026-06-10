// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

fn main() {
    println!("cargo::rustc-check-cfg=cfg(gstd_nightly)");

    if rustc_version::version_meta()
        .is_ok_and(|meta| meta.channel == rustc_version::Channel::Nightly)
    {
        println!("cargo::rustc-cfg=gstd_nightly");
    }
}
