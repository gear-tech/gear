// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! gbuild utils

use colored::Colorize;
use std::process;

/// Error print and exit process
pub fn error(error: &[u8]) {
    eprint!(
        "{}: {}",
        "error".red().bold(),
        String::from_utf8_lossy(error)
    );
    process::exit(1);
}

/// Prints info with green title
pub fn info(title: &str, info: &str) {
    println!("{:>12} {}", title.green().bold(), info);
}
