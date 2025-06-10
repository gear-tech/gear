// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use jsonrpsee::types::ErrorObject;

// TODO #4364: https://github.com/gear-tech/gear/issues/4364

pub fn db(err: &'static str) -> ErrorObject<'static> {
    ErrorObject::owned(8000, "Database error", Some(err))
}

pub fn runtime(err: impl ToString) -> ErrorObject<'static> {
    ErrorObject::owned(8000, "Runtime error", Some(err.to_string()))
}

pub fn internal() -> ErrorObject<'static> {
    ErrorObject::owned(8000, "Internal error", None::<&str>)
}

pub fn tx_pool(err: anyhow::Error) -> ErrorObject<'static> {
    ErrorObject::owned(8000, "Transaction pool error", Some(format!("{err}")))
}
