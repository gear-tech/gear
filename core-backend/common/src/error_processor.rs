// This file is part of Gear.
//
// Copyright (C) 2022 Gear Technologies Inc.
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

use gear_core_errors::ExtError;

pub struct ExtErrorProcessor<T> {
    inner: Result<T, ExtError>,
}

impl<T> ExtErrorProcessor<T> {
    fn new<E>(result: Result<T, E>) -> Result<Self, E>
    where
        E: IntoExtError,
    {
        match result {
            Ok(t) => Ok(Self { inner: Ok(t) }),
            Err(err) => {
                let err = err.into_ext_error()?;
                Ok(Self { inner: Err(err) })
            }
        }
    }

    pub fn proc_res<F, E, EO>(self, f: F) -> Result<(), EO>
    where
        // TODO: use here NonZeroU32
        F: FnOnce(Result<T, u32>) -> Result<(), E>,
        EO: From<E>,
    {
        match self.inner {
            Ok(t) => f(Ok(t))?,
            Err(err) => f(Err(err.encoded_size() as u32))?,
        }

        Ok(())
    }
}

impl ExtErrorProcessor<()> {
    pub fn error_len(self) -> u32 {
        self.inner
            .err()
            .map(|err| err.encoded_size() as u32)
            .unwrap_or(0)
    }
}

pub trait ProcessError<T, E>: Sized {
    fn process_error(self) -> Result<ExtErrorProcessor<T>, E>;
}

impl<T, E: IntoExtError> ProcessError<T, E> for Result<T, E> {
    fn process_error(self) -> Result<ExtErrorProcessor<T>, E> {
        ExtErrorProcessor::new(self)
    }
}

pub trait IntoExtError: Sized {
    fn into_ext_error(self) -> Result<ExtError, Self>;
}
