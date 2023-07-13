// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

#![no_std]

extern crate alloc;

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::fmt::{self, Debug, Display, Formatter};
use wabt::{Wasm2Wat, Wat2Wasm};

pub struct WatStr(String);

pub enum WatExample {
    Custom(WatStr),
    InfRecursion,
    LargeScheduled,
    ReadAccess,
    ReadWriteAccess,
    WrongLoad,
}

impl Debug for WatExample {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "\nWatExample::{}:\n{}", self.name(), self.wat())
    }
}

impl Display for WatExample {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "WatExample::{}: (/ .. /)", self.name())
    }
}

impl WatExample {
    const VALIDATION: bool = true;

    fn name(&self) -> &'static str {
        match self {
            Self::Custom(_) => "Custom",
            Self::InfRecursion => "InfRecursion",
            Self::LargeScheduled => "LargeScheduled",
            Self::ReadAccess => "ReadAccess",
            Self::ReadWriteAccess => "ReadWriteAccess",
            Self::WrongLoad => "WrongLoad",
        }
    }

    pub fn from_wat(wat: impl AsRef<str>) -> Option<Self> {
        let wat = wat.as_ref();

        Wat2Wasm::new()
            .validate(Self::VALIDATION)
            .convert(wat)
            .map(|_| Self::Custom(WatStr(wat.to_string())))
            .ok()
    }

    pub fn from_code(code: impl AsRef<[u8]>) -> Option<Self> {
        Wasm2Wat::new().convert(code).ok().and_then(|wat| {
            String::from_utf8(wat.as_ref().to_vec())
                .ok()
                .map(|wat| Self::Custom(WatStr(wat)))
        })
    }

    pub fn from_hex(hex: impl AsRef<str>) -> Option<Self> {
        let hex = hex.as_ref().trim_start_matches("0x");
        let code = hex::decode(hex).ok()?;

        Self::from_code(code)
    }

    pub fn wat(&'_ self) -> &'_ str {
        match self {
            Self::InfRecursion => include_str!("../spec/inf_recursion.wat"),
            Self::LargeScheduled => include_str!("../spec/large_scheduled.wat"),
            Self::ReadAccess => include_str!("../spec/read_access.wat"),
            Self::ReadWriteAccess => include_str!("../spec/read_write_access.wat"),
            Self::WrongLoad => include_str!("../spec/wrong_load.wat"),
            Self::Custom(WatStr(string)) => string.as_ref(),
        }
    }

    pub fn code(&self) -> Vec<u8> {
        Wat2Wasm::new()
            .validate(Self::VALIDATION)
            .convert(self.wat())
            .expect("Failed to parse module")
            .as_ref()
            .to_vec()
    }
}
