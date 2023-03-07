// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Data access synchronization objects.
//!
//! These synchronization objects are similar to those in the [`std::sync`](https://doc.rust-lang.org/std/sync/) module, but they prevent data races when dealing with multiple actors (users or programs) instead of multiple threads considered in classic synchronization objects.
//!
//! The following is an overview of the available synchronization objects:
//!
//! - [`Mutex`](self::Mutex): The Mutual Exclusion mechanism guarantees that
//!   during execution, only a single actor can access data at any given time.
//! - [`RwLock`](self::RwLock): Provides a mutual exclusion mechanism that
//!   allows multiple readings by different actors while allowing only one
//!   writer at the execution. In some cases, this can be more efficient than a
//!   mutex.

mod access;

mod mutex;
mod rwlock;

pub use self::{
    mutex::{Mutex, MutexGuard, MutexLockFuture},
    rwlock::{RwLock, RwLockReadFuture, RwLockReadGuard, RwLockWriteFuture, RwLockWriteGuard},
};
