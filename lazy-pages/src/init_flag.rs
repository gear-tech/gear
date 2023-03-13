// This file is part of Gear.
//
// Copyright (C) 2023 Gear Technologies Inc.
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

#[cfg(not(test))]
mod not_tests {
    use crate::InitError;
    use once_cell::sync::OnceCell;

    pub struct InitializationFlag(OnceCell<Result<(), InitError>>);

    impl InitializationFlag {
        pub const fn new() -> Self {
            Self(OnceCell::new())
        }

        pub fn get_or_init(
            &self,
            f: impl FnOnce() -> Result<(), InitError>,
        ) -> Result<(), InitError> {
            self.0.get_or_init(f).clone()
        }
    }
}

#[cfg(not(test))]
pub use not_tests::*;

#[cfg(test)]
mod tests {
    use crate::InitError;
    use std::sync::Mutex;

    pub struct InitializationFlag(Mutex<Option<Result<(), InitError>>>);

    impl InitializationFlag {
        pub const fn new() -> Self {
            Self(Mutex::new(None))
        }

        pub fn get_or_init(
            &self,
            f: impl FnOnce() -> Result<(), InitError>,
        ) -> Result<(), InitError> {
            self.0.lock().unwrap().get_or_insert(f()).clone()
        }

        pub fn reset(&self) {
            self.0.lock().unwrap().take();
        }
    }
}

#[cfg(test)]
pub use tests::*;
