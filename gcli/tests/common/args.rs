// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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

/// Command line args
#[derive(Clone)]
pub struct Args {
    endpoint: Option<String>,
    command: String,
    args: Vec<(String, String)>,
    with: Vec<String>,
}

impl Args {
    /// New Args.
    pub fn new(command: impl ToString) -> Self {
        Self {
            endpoint: None,
            command: command.to_string(),
            args: vec![],
            with: vec![],
        }
    }

    /// Append endpoint.
    pub fn endpoint(mut self, endpoint: impl ToString) -> Self {
        self.endpoint = Some(endpoint.to_string());
        self
    }
}

impl From<Args> for Vec<String> {
    fn from(args: Args) -> Self {
        vec![
            if let Some(endpoint) = args.endpoint {
                vec!["--endpoint".into(), endpoint]
            } else {
                vec![]
            },
            vec![args.command.to_string()],
            args.args
                .into_iter()
                .map(|(f, a)| [f, a])
                .collect::<Vec<[String; 2]>>()
                .concat(),
            args.with,
        ]
        .concat()
    }
}

macro_rules! impl_args {
    ([$($flag:tt),+], [$($value:tt),+]) => {
        impl Args {
            $(
                pub fn $flag(mut self, value: impl ToString) -> Self {
                    self.args.push((
                        "--".to_string() + &stringify!($flag).replace("_", "-"),
                        value.to_string(),
                    ));
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
    [payload, gas_limit, value],
    [
        program,
        message_id,
        address,
        action,
        destination,
        amount,
        meta,
        flag,
        derive
    ]
);
