// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Module for toggler/flag implementation.
//!
//! Toggler provides API for allowing or denying actions.
//! Could be used to branch logic by toggler condition.

use crate::storage::ValueStorage;
use core::marker::PhantomData;

/// Represents logic of providing access for some actions.
pub trait Toggler {
    /// Sets condition to allowed for some action.
    fn allow();

    /// Returns bool, defining does toggle allow some action.
    fn allowed() -> bool;

    /// Returns bool, defining does toggle deny some action.
    ///
    /// Represents `Self::allowed` inversion.
    fn denied() -> bool {
        !Self::allowed()
    }

    /// Sets condition to denied for some action.
    fn deny();
}

/// `Toggler` implementation based on `ValueStorage`.
pub struct TogglerImpl<VS: ValueStorage>(PhantomData<VS>);

// `Toggler` implementation over `ValueStorage` of `bool` storing type.
impl<VS: ValueStorage<Value = bool>> Toggler for TogglerImpl<VS> {
    fn allow() {
        VS::put(true);
    }

    fn allowed() -> bool {
        VS::get() != Some(false)
    }

    fn deny() {
        VS::put(false);
    }
}
