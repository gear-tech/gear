// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
