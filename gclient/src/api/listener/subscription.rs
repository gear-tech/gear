// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use super::EventProcessor;
use crate::{Error, Result};
use async_trait::async_trait;
use gsdk::metadata::{runtime_types::gear_runtime::RuntimeEvent, Event};

#[async_trait(?Send)]
impl<I: IntoIterator<Item = RuntimeEvent> + Clone> EventProcessor for I {
    fn not_waited() -> Error {
        Error::EventNotFoundInIterator
    }

    async fn proc<T>(&mut self, predicate: impl Fn(Event) -> Option<T>) -> Result<T> {
        let mut res = None;

        for event in self.clone().into_iter() {
            if let Some(data) = predicate(event.into()) {
                res = res.or_else(|| Some(data));
            }

            if res.is_some() {
                break;
            }
        }

        res.ok_or_else(Self::not_waited)
    }

    async fn proc_many<T>(
        &mut self,
        predicate: impl Fn(Event) -> Option<T>,
        validate: impl Fn(Vec<T>) -> (Vec<T>, bool),
    ) -> Result<Vec<T>> {
        let mut res = vec![];

        for event in self.clone().into_iter() {
            if let Some(data) = predicate(event.into()) {
                res.push(data);
            }
        }

        if let (res, true) = validate(res) {
            Ok(res)
        } else {
            Err(Self::not_waited())
        }
    }
}
