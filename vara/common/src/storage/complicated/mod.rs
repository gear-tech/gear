// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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
