// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use jsonrpsee::types::{ErrorObject, error::INVALID_PARAMS_CODE};

// TODO #4364: https://github.com/gear-tech/gear/issues/4364

pub fn db(err: &'static str) -> ErrorObject<'static> {
    ErrorObject::owned(8000, "Database error", Some(err))
}

pub fn runtime(err: impl ToString) -> ErrorObject<'static> {
    ErrorObject::owned(8000, "Runtime error", Some(err.to_string()))
}

pub fn bad_request(err: impl ToString) -> ErrorObject<'static> {
    ErrorObject::owned(8000, "Bad request", Some(err.to_string()))
}

pub fn internal() -> ErrorObject<'static> {
    ErrorObject::owned(8000, "Internal error", None::<&str>)
}

pub fn invalid_params(err: impl ToString) -> ErrorObject<'static> {
    ErrorObject::owned(INVALID_PARAMS_CODE, "Invalid params", Some(err.to_string()))
}
