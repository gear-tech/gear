// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Useful things for generating code.

use std::{
    io::Write,
    process::{Command, Stdio},
};

/// License header.
pub const LICENSE: &str = r#"
// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

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
