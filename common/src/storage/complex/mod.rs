// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
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
