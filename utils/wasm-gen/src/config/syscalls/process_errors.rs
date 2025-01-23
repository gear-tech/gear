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

//! Processing syscalls errors config.

use std::collections::HashSet;

use crate::InvocableSyscall;

#[derive(Debug, Clone, Default)]
pub enum ErrorProcessingConfig {
    /// Process errors on all the fallible syscalls.
    All,
    /// Process only errors on provided syscalls.
    Whitelist(ErrorProneSyscalls),
    /// Process errors on all the syscalls excluding provided.
    Blacklist(ErrorProneSyscalls),
    /// Don't process syscall errors at all.
    #[default]
    None,
}

impl ErrorProcessingConfig {
    pub fn error_should_be_processed(&self, syscall: InvocableSyscall) -> bool {
        match self {
            Self::All => true,
            Self::Whitelist(wl) => wl.contains(syscall),
            Self::Blacklist(bl) => {
                if syscall.returns_error() {
                    !bl.contains(syscall)
                } else {
                    false
                }
            }
            Self::None => false,
        }
    }
}

/// Set of syscalls that return an error.
///
/// Basically, it's a wrapper over a hash set of [`InvocableSyscall`],
/// that controls types of inserted syscalls.
#[derive(Debug, Clone, Default)]
pub struct ErrorProneSyscalls(HashSet<InvocableSyscall>);

impl ErrorProneSyscalls {
    /// Create an empty set of returning error syscalls.
    pub fn new() -> Self {
        Self(HashSet::new())
    }

    /// Insert an error-prone syscall into the set.
    pub fn insert(&mut self, syscall: InvocableSyscall) {
        if syscall.returns_error() {
            self.0.insert(syscall);
        } else {
            panic!(
                "{syscall_str} is neither fallible, nor returns error value.",
                syscall_str = syscall.to_str()
            );
        }
    }

    /// Check if the `syscall` is in the set.
    pub fn contains(&self, syscall: InvocableSyscall) -> bool {
        self.0.contains(&syscall)
    }
}
