// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

// WASM page has size of 64KiBs (65_536 bytes)
pub const PAGE_SIZE: u32 = 0x10000;

#[derive(Clone, Copy, Debug, Default)]
pub struct Page(u16);

impl From<u16> for Page {
    fn from(value: u16) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PageCount(u32);

impl From<u32> for PageCount {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<Page> for PageCount {
    fn from(value: Page) -> Self {
        Self(u32::from(value.0) + 1)
    }
}

impl PageCount {
    pub fn size(&self) -> u32 {
        self.0 * PAGE_SIZE
    }

    pub fn raw_number(&self) -> u32 {
        self.0
    }
}
