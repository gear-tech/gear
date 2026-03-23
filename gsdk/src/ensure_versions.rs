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

//! This module ensures that the crate uses the save
//! versions of libraries as [`subxt`].
//!
//! This is done by ensuring that the same types
//! from the external crate and its reexport by
//! [`subxt`] are the same type.

#![allow(unused)]

#[diagnostic::on_unimplemented(message = "Types `{Self}` and `{A}` are not the same type")]
trait SameType<A> {}

impl<T> SameType<T> for T {}

const fn ensure_same_type<A, B>()
where
    B: SameType<A>,
{
}

const _: () = {
    ensure_same_type::<jsonrpsee::core::client::Client, subxt::ext::jsonrpsee::core::client::Client>(
    );
    ensure_same_type::<parity_scale_codec::DecodeFinished, subxt::ext::codec::DecodeFinished>();
    ensure_same_type::<url::Url, jsonrpsee::client_transport::ws::Url>();
};
