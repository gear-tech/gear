// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![no_std]

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

pub const CHILD_WAT: &str = r#"
(module
    (type (;0;) (func))
    (import "env" "memory" (memory (;0;) 1))
    (func (;0;) (type 0))
    (func (;1;) (type 0))
    (export "handle" (func 0))
    (export "init" (func 1))
)
"#;

#[cfg(not(feature = "std"))]
mod wasm;
