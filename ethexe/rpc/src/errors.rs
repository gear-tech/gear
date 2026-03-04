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

use jsonrpsee::types::{
    ErrorCode, ErrorObject,
    error::{CALL_EXECUTION_FAILED_CODE, INTERNAL_ERROR_CODE, INVALID_REQUEST_CODE},
};
use serde::Serialize;

pub fn runtime<S>(message: impl ToString, data: Option<S>) -> ErrorObject<'static>
where
    S: Serialize,
{
    ErrorObject::owned(CALL_EXECUTION_FAILED_CODE, message, data)
}

pub fn bad_request(err: impl ToString) -> ErrorObject<'static> {
    ErrorObject::owned(INVALID_REQUEST_CODE, "Bad request", Some(err.to_string()))
}

pub fn internal<S: Serialize>(message: impl ToString, data: Option<S>) -> ErrorObject<'static> {
    ErrorObject::owned(INTERNAL_ERROR_CODE, message, data)
}
