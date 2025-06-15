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

//! Command line args builder

use std::fmt;

/// Command line args
#[derive(Clone)]
pub struct Args {
    pub endpoint: Option<String>,
    pub command: String,
    pub args: Vec<String>,
    pub with: Vec<String>,
    pub stdin: Vec<u8>,
}

impl<const N: usize> From<[&'static str; N]> for Args {
    fn from(value: [&str; N]) -> Self {
        let command = value[0];
        let with = value[1..].iter().map(|s| s.to_string()).collect();

        let mut args = Args::new(command);
        args.with = with;
        args
    }
}

impl Args {
    /// New Args.
    pub fn new(command: impl ToString) -> Self {
        Self {
            endpoint: None,
            command: command.to_string(),
            args: vec![],
            with: vec![],
            stdin: vec![],
        }
    }

    /// Append endpoint.
    pub fn endpoint(mut self, endpoint: impl ToString) -> Self {
        self.endpoint = Some(endpoint.to_string());
        self
    }

    pub fn program_stdin(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.with.push("-".to_string());
        self.stdin = bytes.into();
        self
    }
}

macro_rules! impl_args {
    (
        flags: $($flag:tt),+;
        values: $($value:tt),+;
    ) => {
        // FIXME: maybe some unused actually must be used
        #[allow(unused)]
        impl Args {
            $(
                pub fn $flag(mut self, value: impl fmt::Display) -> Self {
                    self.args.push(format!("--{flag}={value}", flag = stringify!($flag).replace("_", "-")));
                    self
                }
            )*

            $(
                pub fn $value(mut self, value: impl ToString) -> Self {
                    self.with.push(value.to_string());
                    self
                }
            )*
        }
    };
}

impl_args!(
    flags: payload, gas_limit, value;
    values:
        message_id,
        address,
        action,
        destination,
        amount,
        meta,
        flag,
        derive;
);
