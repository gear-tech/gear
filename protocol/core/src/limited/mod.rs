// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! This module provides container types with statically limited length.

mod str;
mod vec;

pub use str::{LimitedStr, LimitedStrError};
pub use vec::{LimitedVec, LimitedVecError};

mod private {
    use core::marker::PhantomData;

    /// Visitor type for manual [`scale_decode::DecodeAsType`]
    /// implementations for limited collections.
    pub struct LimitedVisitor<T, R>(PhantomData<(T, R)>);

    impl<T, R> LimitedVisitor<T, R> {
        pub(crate) const DEFAULT: Self = Self(PhantomData);
    }
}
