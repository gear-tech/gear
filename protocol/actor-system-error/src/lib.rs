// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Actor-system error.
//!
//! Actor is intended to be errors passed to user.
//! System errors are to be unreachable or recoverable.

#![no_std]

/// Define type alias with implemented `From`s.
#[macro_export]
macro_rules! actor_system_error {
    (
        $(#[$($meta:meta)*])?
        $vis:vis type $name:ident = ActorSystemError<$actor_err:ident, $system_err:ident>;
    ) => {
        $(#[$($meta)*])?
        $vis type $name = $crate::ActorSystemError<$actor_err, $system_err>;

        impl From<$actor_err> for $crate::ActorSystemError<$actor_err, $system_err> {
            fn from(err: $actor_err) -> Self {
                Self::Actor(err)
            }
        }

        impl From<$system_err> for $crate::ActorSystemError<$actor_err, $system_err> {
            fn from(err: $system_err) -> Self {
                Self::System(err)
            }
        }
    };
}

/// Actor or system error.
#[derive(Debug, Eq, PartialEq, derive_more::Display)]
pub enum ActorSystemError<A, S> {
    Actor(A),
    System(S),
}

impl<A, S> ActorSystemError<A, S> {
    /// Map actor error.
    pub fn map_actor<F, U>(self, f: F) -> ActorSystemError<U, S>
    where
        F: FnOnce(A) -> U,
    {
        match self {
            Self::Actor(a) => ActorSystemError::Actor(f(a)),
            Self::System(s) => ActorSystemError::System(s),
        }
    }

    /// Map system error.
    pub fn map_system<F, U>(self, f: F) -> ActorSystemError<A, U>
    where
        F: FnOnce(S) -> U,
    {
        match self {
            Self::Actor(a) => ActorSystemError::Actor(a),
            Self::System(s) => ActorSystemError::System(f(s)),
        }
    }

    /// Map actor or system error using [`From::from()`].
    pub fn map_into<UA, US>(self) -> ActorSystemError<UA, US>
    where
        UA: From<A>,
        US: From<S>,
    {
        match self {
            Self::Actor(a) => ActorSystemError::Actor(UA::from(a)),
            Self::System(s) => ActorSystemError::System(US::from(s)),
        }
    }
}

/// Extension for [`Result`] around actor-system error.
pub trait ResultExt<T, A, S> {
    /// Map actor error.
    fn map_actor_err<F, U>(self, f: F) -> Result<T, ActorSystemError<U, S>>
    where
        F: FnOnce(A) -> U;

    /// Map system error.
    fn map_system_err<F, U>(self, f: F) -> Result<T, ActorSystemError<A, U>>
    where
        F: FnOnce(S) -> U;

    /// Map actor or system error.
    fn map_err_into<UA, US>(self) -> Result<T, ActorSystemError<UA, US>>
    where
        UA: From<A>,
        US: From<S>;
}

impl<T, A, S> ResultExt<T, A, S> for Result<T, ActorSystemError<A, S>> {
    fn map_actor_err<F, U>(self, f: F) -> Result<T, ActorSystemError<U, S>>
    where
        F: FnOnce(A) -> U,
    {
        self.map_err(|err| err.map_actor(f))
    }

    fn map_system_err<F, U>(self, f: F) -> Result<T, ActorSystemError<A, U>>
    where
        F: FnOnce(S) -> U,
    {
        self.map_err(|err| err.map_system(f))
    }

    fn map_err_into<UA, US>(self) -> Result<T, ActorSystemError<UA, US>>
    where
        UA: From<A>,
        US: From<S>,
    {
        self.map_err(ActorSystemError::map_into)
    }
}
