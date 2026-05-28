// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use derive_more::Display;
use wasmi::core::HostError;

#[derive(Debug, Display)]
pub struct CustomHostError {
    message: String,
}

impl HostError for CustomHostError {}

impl<T> From<T> for CustomHostError
where
    T: Into<String>,
{
    fn from(s: T) -> CustomHostError {
        Self { message: s.into() }
    }
}
