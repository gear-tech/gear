// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Module for map's key generation primitives.
//!
//! Key generator primitives might be used to prevent
//! manual specifying of the key for cases,
//! when data stored in map.

use crate::Origin;
use core::marker::PhantomData;
use gear_core::{
    ids::{ActorId, MessageId},
    message::{StoredDispatch, UserStoredMessage},
};

/// Represents logic of providing key as specified
/// (associated) type for given as specified
/// (associated) type value by reference.
pub trait KeyFor {
    /// Generated key type.
    type Key;
    /// Value over which key should be generated type.
    type Value;

    /// Generates key for given by reference Value.
    fn key_for(value: &Self::Value) -> Self::Key;
}

/// Key generator for `gear`'s mailbox implementation.
pub struct MailboxKeyGen<T>(PhantomData<T>);

// `MailboxKeyGen` stores `StoredMessage` under it's
// `MessageId` id parameter and the generic `T: Origin`
// (represented with `Substrate`'s 32-byte `AccountId`)
// destination parameter.
impl<T: Origin> KeyFor for MailboxKeyGen<T> {
    type Key = (T, MessageId);
    type Value = UserStoredMessage;

    fn key_for(value: &Self::Value) -> Self::Key {
        (value.destination().cast(), value.id())
    }
}

/// Key generator for `gear`'s message queue implementation.
pub struct QueueKeyGen;

// `QueueKeyGen` stores `StoredDispatch` under
// it's `MessageId` id parameter.
impl KeyFor for QueueKeyGen {
    type Key = MessageId;
    type Value = StoredDispatch;

    fn key_for(value: &Self::Value) -> Self::Key {
        value.id()
    }
}

/// Key generator for `gear`'s waitlist implementation.
pub struct WaitlistKeyGen;

// `WaitlistKeyGen` stores `StoredDispatch` under it's destination
// `ActorId` and `MessageId` id parameters.
impl KeyFor for WaitlistKeyGen {
    type Key = (ActorId, MessageId);
    type Value = StoredDispatch;

    fn key_for(value: &Self::Value) -> Self::Key {
        (value.destination(), value.id())
    }
}
