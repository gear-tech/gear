// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use ethexe_common::db::OnChainStorageRO;
use ethexe_db::Database;
use std::fmt;

pub(crate) mod discovery;
pub(crate) mod list;
pub(crate) mod topic;

#[auto_impl::auto_impl(&, Box)]
pub trait ValidatorDatabase: Send + fmt::Debug + OnChainStorageRO {
    fn clone_boxed(&self) -> Box<dyn ValidatorDatabase>;
}

impl ValidatorDatabase for Database {
    fn clone_boxed(&self) -> Box<dyn ValidatorDatabase> {
        Box::new(self.clone())
    }
}
