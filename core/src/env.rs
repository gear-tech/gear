// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

//! Environment for running a module.

use alloc::rc::Rc;
use core::cell::RefCell;

use anyhow::Result;
use codec::{Decode, Encode};

use crate::memory::PageNumber;
use crate::message::{ExitCode, MessageId, OutgoingPacket, ReplyPacket};
use crate::program::ProgramId;

/// Page access rights.
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, Copy)]
pub enum PageAction {
    /// Can be read.
    Read,
    /// Can be written.
    Write,
    /// No access.
    None,
}

/// External api for managing memory, messages, allocations and gas-counting.
pub trait Ext {
    /// Allocate number of pages.
    ///
    /// The resulting page number should point to `pages` consecutives memory pages.
    fn alloc(&mut self, pages: PageNumber) -> Result<PageNumber, &'static str>;

    /// Get the current block height.
    fn block_height(&self) -> u32;

    /// Get the current block timestamp.
    fn block_timestamp(&self) -> u64;

    /// Initialize a new incomplete message for another program and return its handle.
    fn send_init(&mut self) -> Result<usize, &'static str>;

    /// Push an extra buffer into message payload by handle.
    fn send_push(&mut self, handle: usize, buffer: &[u8]) -> Result<(), &'static str>;

    /// Complete message and send it to another program.
    fn send_commit(
        &mut self,
        handle: usize,
        msg: OutgoingPacket,
    ) -> Result<MessageId, &'static str>;

    /// Send message to another program.
    fn send(&mut self, msg: OutgoingPacket) -> Result<MessageId, &'static str> {
        let handle = self.send_init()?;
        self.send_commit(handle, msg)
    }

    /// Push an extra buffer into reply message.
    fn reply_push(&mut self, buffer: &[u8]) -> Result<(), &'static str>;

    /// Complete reply message and send it to source program.
    fn reply_commit(&mut self, msg: ReplyPacket) -> Result<MessageId, &'static str>;

    /// Produce reply to the current message.
    fn reply(&mut self, msg: ReplyPacket) -> Result<MessageId, &'static str> {
        self.reply_commit(msg)
    }

    /// Read the message id, if current message is a reply.
    fn reply_to(&self) -> Option<(MessageId, ExitCode)>;

    /// Get the source of the message currently being handled.
    fn source(&mut self) -> ProgramId;

    /// Get the id of the message currently being handled.
    fn message_id(&mut self) -> MessageId;

    /// Free specific memory page.
    ///
    /// Unlike traditional allocator, if multiple pages allocated via `alloc`, all pages
    /// should be `free`-d separately.
    fn free(&mut self, ptr: PageNumber) -> Result<(), &'static str>;

    /// Send debug message.
    ///
    /// This should be no-op in release builds.
    fn debug(&mut self, data: &str) -> Result<(), &'static str>;

    /// Set memory region at specific pointer.
    fn set_mem(&mut self, ptr: usize, val: &[u8]);

    /// Reads memory contents at the given offset into a buffer.
    fn get_mem(&self, ptr: usize, buffer: &mut [u8]);

    /// Access currently handled message payload.
    fn msg(&mut self) -> &[u8];

    /// Charge some gas.
    fn charge_gas(&mut self, amount: u32) -> Result<(), &'static str>;

    /// Refund some gas.
    fn refund_gas(&mut self, amount: u32) -> Result<(), &'static str>;

    /// Tell how much gas is left in running context.
    fn gas_available(&mut self) -> u64;

    /// Value associated with message.
    fn value(&self) -> u128;

    /// Interrupt the program and reschedule execution.
    fn wait(&mut self) -> Result<(), &'static str>;

    /// Wake the waiting message and move it to the processing queue.
    fn wake(&mut self, waker_id: MessageId) -> Result<(), &'static str>;
}

/// Struct for interacting with Ext
#[derive(Default)]
pub struct LaterExt<E: Ext> {
    inner: Rc<RefCell<Option<E>>>,
}

impl<E: Ext> Clone for LaterExt<E> {
    fn clone(&self) -> Self {
        Self {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<E: Ext> LaterExt<E> {
    /// Create empty ext
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(None)),
        }
    }

    /// Set ext
    pub fn set(&mut self, e: E) {
        *self.inner.borrow_mut() = Some(e)
    }

    /// Call fn with inner ext
    pub fn with<R>(&self, f: impl FnOnce(&mut E) -> R) -> Result<R, &'static str> {
        self.with_fallible(|e| Ok(f(e)))
    }

    /// Call fn with inner ext
    pub fn with_fallible<R>(
        &self,
        f: impl FnOnce(&mut E) -> Result<R, &'static str>,
    ) -> Result<R, &'static str> {
        let mut brw = self.inner.borrow_mut();
        let mut ext = brw
            .take()
            .ok_or("with should be called only when inner is set")?;
        let res = f(&mut ext);

        *brw = Some(ext);

        res
    }

    /// Unset inner ext
    pub fn unset(&mut self) -> E {
        self.inner
            .borrow_mut()
            .take()
            .expect("Unset should be paired with set and called after")
    }
}

#[cfg(test)]
/// This module contains tests of interacting with LaterExt
mod tests {
    use super::*;

    /// Struct with internal value to interact with LaterExt
    #[derive(Debug, PartialEq)]
    struct ExtImplementedStruct(u8);

    /// Empty Ext implementation for test struct
    impl Ext for ExtImplementedStruct {
        fn alloc(&mut self, _pages: PageNumber) -> Result<PageNumber, &'static str> {
            Err("")
        }
        fn block_height(&self) -> u32 {
            0
        }
        fn block_timestamp(&self) -> u64 {
            0
        }
        fn send_init(&mut self) -> Result<usize, &'static str> {
            Ok(0)
        }
        fn send_push(&mut self, _handle: usize, _buffer: &[u8]) -> Result<(), &'static str> {
            Ok(())
        }
        fn reply_commit(&mut self, _msg: ReplyPacket) -> Result<MessageId, &'static str> {
            Ok(MessageId::default())
        }
        fn reply_push(&mut self, _buffer: &[u8]) -> Result<(), &'static str> {
            Ok(())
        }
        fn send_commit(
            &mut self,
            _handle: usize,
            _msg: OutgoingPacket,
        ) -> Result<MessageId, &'static str> {
            Ok(MessageId::default())
        }
        fn reply_to(&self) -> Option<(MessageId, ExitCode)> {
            None
        }
        fn source(&mut self) -> ProgramId {
            ProgramId::from(0)
        }
        fn message_id(&mut self) -> MessageId {
            0.into()
        }
        fn free(&mut self, _ptr: PageNumber) -> Result<(), &'static str> {
            Ok(())
        }
        fn debug(&mut self, _data: &str) -> Result<(), &'static str> {
            Ok(())
        }
        fn set_mem(&mut self, _ptr: usize, _val: &[u8]) {}
        fn get_mem(&self, _ptr: usize, _buffer: &mut [u8]) {}
        fn msg(&mut self) -> &[u8] {
            &[]
        }
        fn charge_gas(&mut self, _amount: u32) -> Result<(), &'static str> {
            Ok(())
        }
        fn refund_gas(&mut self, _amount: u32) -> Result<(), &'static str> {
            Ok(())
        }
        fn gas_available(&mut self) -> u64 {
            1_000_000
        }
        fn value(&self) -> u128 {
            0
        }
        fn wait(&mut self) -> Result<(), &'static str> {
            Ok(())
        }
        fn wake(&mut self, _waker_id: MessageId) -> Result<(), &'static str> {
            Ok(())
        }
    }

    #[test]
    /// Test that the new LaterExt object contains reference on None value
    fn empty_ext_creation() {
        let ext = LaterExt::<ExtImplementedStruct>::new();

        assert_eq!(ext.inner, Rc::new(RefCell::new(None)));
    }

    #[test]
    /// Test that we are able to set and unset LaterExt value
    fn setting_and_unsetting_inner_ext() {
        let mut ext = LaterExt::<ExtImplementedStruct>::new();

        ext.set(ExtImplementedStruct(0));

        assert_eq!(
            ext.inner,
            Rc::new(RefCell::new(Some(ExtImplementedStruct(0))))
        );

        let inner = ext.unset();

        assert_eq!(inner, ExtImplementedStruct(0));
        assert_eq!(ext.inner, Rc::new(RefCell::new(None)));

        ext.set(ExtImplementedStruct(0));
        // When we set a new value, the previous one is reset
        ext.set(ExtImplementedStruct(1));

        let inner = ext.unset();

        assert_eq!(inner, ExtImplementedStruct(1));
        assert_eq!(ext.inner, Rc::new(RefCell::new(None)));
    }

    #[test]
    #[should_panic(expected = "Unset should be paired with set and called after")]
    /// Test that unsetting an empty value causes panic
    fn unsetting_empty_ext() {
        let mut ext = LaterExt::<ExtImplementedStruct>::new();

        let _ = ext.unset();
    }

    #[test]
    /// Test that ext's clone still refers to the same inner object as the original one
    fn ext_cloning() {
        let mut ext_source = LaterExt::<ExtImplementedStruct>::new();
        let mut ext_clone = ext_source.clone();

        // ext_clone refers the same inner as ext_source,
        // so setting on one causes setting on other
        ext_source.set(ExtImplementedStruct(0));

        let inner = ext_clone.unset();

        assert_eq!(inner, ExtImplementedStruct(0));
    }

    /// Test function of format `Fn(&mut E: Ext) -> R`
    /// to call `fn with<R>(&self, f: impl FnOnce(&mut E) -> R) -> R`.
    /// For example, returns the field of ext's inner value.
    fn converter(e: &mut ExtImplementedStruct) -> u8 {
        e.0
    }

    #[test]
    /// Test that ext's `with<R>(...)` works correct when the inner is set
    fn calling_fn_with_inner_ext() {
        let mut ext = LaterExt::<ExtImplementedStruct>::new();
        ext.set(ExtImplementedStruct(0));

        let converted_inner = ext.with(converter);

        assert!(converted_inner.is_ok());
    }

    #[test]
    /// Test that calling ext's `with<R>(...)` throws error
    /// when the inner value was not set or was unsetted
    fn calling_fn_with_empty_ext() {
        let ext = LaterExt::<ExtImplementedStruct>::new();

        assert!(ext.with(converter).is_err());
    }
}
