// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-lat&er WITH Classpath-exception-2.0

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

use gear_backend_common::{BackendExt, BackendState, BackendTermination, TerminationReason};
use gear_core_errors::ExtError;

pub(crate) type HostState<E> = Option<State<E>>;

/// It's supposed that `E` implements [BackendExt]
pub(crate) struct State<E> {
    pub ext: E,
    pub fallible_syscall_error: Option<ExtError>,
    pub termination_reason: TerminationReason,
}

impl<E: BackendExt> BackendTermination<E, ()> for State<E> {
    fn into_parts(self) -> (E, (), TerminationReason) {
        let State {
            ext,
            termination_reason,
            ..
        } = self;
        (ext, (), termination_reason)
    }
}

impl<E> BackendState for State<E> {
    fn set_termination_reason(&mut self, reason: TerminationReason) {
        self.termination_reason = reason;
    }

    fn set_fallible_syscall_error(&mut self, err: ExtError) {
        self.fallible_syscall_error = Some(err);
    }
}
