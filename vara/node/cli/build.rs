// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use substrate_build_script_utils::{generate_cargo_keys, rerun_if_git_head_changed};

fn main() {
    generate_cargo_keys();
    rerun_if_git_head_changed();
}
