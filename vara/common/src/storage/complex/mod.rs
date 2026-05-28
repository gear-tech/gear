// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Gear storage complex types.
//!
//! This module contains finite-logic elements of gear's
//! storage types for gear's runtime.

// Private modules declaration.
mod mailbox;
mod messenger;
mod queue;
mod waitlist;

// Public exports from complex modules.
pub use mailbox::{Mailbox, MailboxCallbacks, MailboxError, MailboxImpl};
pub use messenger::Messenger;
pub use queue::{Queue, QueueImpl};
pub use waitlist::{Waitlist, WaitlistCallbacks, WaitlistError, WaitlistImpl};
