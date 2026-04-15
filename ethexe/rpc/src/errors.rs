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
    ErrorObject,
    error::{
        CALL_EXECUTION_FAILED_CODE, INTERNAL_ERROR_CODE, INTERNAL_ERROR_MSG, INVALID_PARAMS_CODE,
        INVALID_PARAMS_MSG, INVALID_REQUEST_CODE, INVALID_REQUEST_MSG,
    },
};
use serde::Serialize;

// TODO: db errors are cause when we do not found some data in data, so maybe rename it to `not_found`.
pub fn db(err: &'static str) -> ErrorObject<'static> {
    ErrorObject::owned(8000, "Database error", Some(err))
}

pub fn runtime(err: impl ToString) -> ErrorObject<'static> {
    ErrorObject::owned(
        CALL_EXECUTION_FAILED_CODE,
        "Runtime error",
        Some(err.to_string()),
    )
}

pub fn bad_request<D: Serialize>(data: Option<D>) -> ErrorObject<'static> {
    ErrorObject::owned(INVALID_REQUEST_CODE, INVALID_REQUEST_MSG, data)
}

pub fn internal() -> ErrorObject<'static> {
    ErrorObject::owned(INTERNAL_ERROR_CODE, INTERNAL_ERROR_MSG, None::<&str>)
}

#[allow(unused)]
pub fn invalid_params() -> ErrorObject<'static> {
    ErrorObject::owned(INVALID_PARAMS_CODE, INVALID_PARAMS_MSG, None::<&str>)
}

pub fn invalid_params_with<D: Serialize>(data: D) -> ErrorObject<'static> {
    ErrorObject::owned(INVALID_PARAMS_CODE, INVALID_PARAMS_MSG, Some(data))
}
