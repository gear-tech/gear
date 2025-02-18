// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

use core::fmt;
use gear_sandbox::Value;

struct ValueFormatter<'a>(&'a Value);

impl fmt::Display for ValueFormatter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.0 {
            Value::I32(i32) => fmt::Display::fmt(i32, f),
            Value::I64(i64) => fmt::Display::fmt(i64, f),
            Value::F32(f32) => fmt::Display::fmt(f32, f),
            Value::F64(f64) => fmt::Display::fmt(f64, f),
        }
    }
}

struct ArgsFormatter<'a>(&'a [Value]);

impl fmt::Display for ArgsFormatter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut iter = self.0.iter();

        if let Some(value) = iter.next() {
            write!(f, "{}", ValueFormatter(value))?;
        }

        for value in iter {
            write!(f, ", {}", ValueFormatter(value))?;
        }

        Ok(())
    }
}

fn function_name<T>() -> &'static str {
    let s = core::any::type_name::<T>();
    let pos = s.rfind("::").unwrap();
    &s[pos + 2..]
}

pub fn trace_syscall<T>(args: &[Value]) {
    log::trace!(target: "syscalls", "{}({})", function_name::<T>(), ArgsFormatter(args));
}
