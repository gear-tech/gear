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

//! Gear storage complicated types.
//!
//! This module contains more difficult types over gear's storage primitives,
//! which provides API for more specific business-logic
//! with globally shared data.

// Private modules declaration.
mod counter;
mod dequeue;
mod limiter;
mod toggler;

// Public exports from complicated modules.
pub use counter::{Counter, CounterImpl};
pub use dequeue::{
    Dequeue, DequeueCallbacks, DequeueDrainIter, DequeueError, DequeueImpl, DequeueIter, LinkedNode,
};
pub use limiter::{Limiter, LimiterImpl};
pub use toggler::{Toggler, TogglerImpl};
