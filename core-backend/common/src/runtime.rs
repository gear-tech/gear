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

//! Trait that both sandbox and wasmi runtimes must implement.

use crate::{BackendSyscallError, UndefinedTerminationReason};
use gear_core_errors::ExtError as FallibleExtError;

/// Error returned from closure argument in [`Runtime::run_fallible`].
#[derive(Debug, Clone)]
pub enum RunFallibleError {
    UndefinedTerminationReason(UndefinedTerminationReason),
    FallibleExt(FallibleExtError),
}

impl<E> From<E> for RunFallibleError
where
    E: BackendSyscallError,
{
    fn from(err: E) -> Self {
        err.into_run_fallible_error()
    }
}
