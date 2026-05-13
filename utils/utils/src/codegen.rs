// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Useful things for generating code.

use std::{
    io::Write,
    process::{Command, Stdio},
};

/// License header.
pub const LICENSE: &str = r#"
// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

"#;

/// Formats generated code with rustfmt.
pub fn format_with_rustfmt(stream: &[u8]) -> String {
    let raw = String::from_utf8_lossy(stream).to_string();
    let mut rustfmt = Command::new("rustfmt");
    let mut code = rustfmt
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Spawn rustfmt failed");

    code.stdin
        .as_mut()
        .expect("Get stdin of rustfmt failed")
        .write_all(raw.as_bytes())
        .expect("pipe generated code to rustfmt failed");

    let out = code.wait_with_output().expect("Run rustfmt failed").stdout;
    String::from_utf8_lossy(&out).to_string()
}
