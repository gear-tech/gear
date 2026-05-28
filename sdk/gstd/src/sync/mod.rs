// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Data access synchronization objects.
//!
//! These synchronization objects are similar to those in the [`std::sync`](https://doc.rust-lang.org/std/sync/) module, but they prevent data races when dealing with multiple actors (users or programs) instead of multiple threads considered in classic synchronization objects.
//!
//! The following is an overview of the available synchronization objects:
//!
//! - [`Mutex`]: The Mutual Exclusion mechanism guarantees that during
//!   execution, only a single actor can access data at any given time.
//! - [`RwLock`]: Provides a mutual exclusion mechanism that allows multiple
//!   readings by different actors while allowing only one writer at the
//!   execution. In some cases, this can be more efficient than a mutex.

mod access;

mod mutex;
mod rwlock;

pub use self::{
    mutex::{Mutex, MutexGuard, MutexLockFuture},
    rwlock::{RwLock, RwLockReadFuture, RwLockReadGuard, RwLockWriteFuture, RwLockWriteGuard},
};

pub(crate) use self::mutex::MutexId;
