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

use crate::{BackendExternalities, BackendState, BackendTermination, UndefinedTerminationReason};

pub type HostState<Ext, Mem> = Option<State<Ext, Mem>>;

/// It's supposed that `Ext` implements [`BackendExternalities`]
pub struct State<Ext, Mem> {
    pub ext: Ext,
    pub memory: Mem,
    pub termination_reason: UndefinedTerminationReason,
}

impl<Ext: BackendExternalities, Mem> BackendTermination<Ext> for State<Ext, Mem> {
    fn into_parts(self) -> (Ext, UndefinedTerminationReason) {
        let State {
            ext,
            termination_reason,
            ..
        } = self;
        (ext, termination_reason)
    }
}

impl<Ext, Mem> BackendState for State<Ext, Mem> {
    fn set_termination_reason(&mut self, reason: UndefinedTerminationReason) {
        self.termination_reason = reason;
    }
}
