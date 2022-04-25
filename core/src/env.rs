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

//! Environment for running a module.

use crate::costs::RuntimeCosts;
use crate::memory::{Memory, WasmPageNumber};
use crate::{
    ids::{MessageId, ProgramId},
    message::{ExitCode, HandlePacket, InitPacket, ReplyPacket},
};
use alloc::rc::Rc;
use anyhow::Result;
use codec::{Decode, Encode};
use core::cell::RefCell;

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
    fn alloc(
        &mut self,
        pages: WasmPageNumber,
        mem: &mut dyn Memory,
    ) -> Result<WasmPageNumber, &'static str>;

    /// Get the current block height.
    fn block_height(&mut self) -> Result<u32, &'static str>;

    /// Get the current block timestamp.
    fn block_timestamp(&mut self) -> Result<u64, &'static str>;

    /// Get the id of the user who initiated communication with blockchain,
    /// during which, currently processing message was created.
    fn origin(&mut self) -> Result<ProgramId, &'static str>;

    /// Initialize a new incomplete message for another program and return its handle.
    fn send_init(&mut self) -> Result<usize, &'static str>;

    /// Push an extra buffer into message payload by handle.
    fn send_push(&mut self, handle: usize, buffer: &[u8]) -> Result<(), &'static str>;

    /// Complete message and send it to another program.
    fn send_commit(&mut self, handle: usize, msg: HandlePacket) -> Result<MessageId, &'static str>;

    /// Send message to another program.
    fn send(&mut self, msg: HandlePacket) -> Result<MessageId, &'static str> {
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
    fn reply_to(&mut self) -> Result<Option<(MessageId, ExitCode)>, &'static str>;

    /// Get the source of the message currently being handled.
    fn source(&mut self) -> Result<ProgramId, &'static str>;

    /// Terminate the program and transfer all available value to the address.
    fn exit(&mut self, value_destination: ProgramId) -> Result<(), &'static str>;

    /// Get the id of the message currently being handled.
    fn message_id(&mut self) -> Result<MessageId, &'static str>;

    /// Get the id of program itself
    fn program_id(&mut self) -> Result<ProgramId, &'static str>;

    /// Free specific memory page.
    ///
    /// Unlike traditional allocator, if multiple pages allocated via `alloc`, all pages
    /// should be `free`-d separately.
    fn free(&mut self, page: WasmPageNumber) -> Result<(), &'static str>;

    /// Send debug message.
    ///
    /// This should be no-op in release builds.
    fn debug(&mut self, data: &str) -> Result<(), &'static str>;

    /// Interrupt the program, saving it's state.
    fn leave(&mut self) -> Result<(), &'static str>;

    /// Access currently handled message payload.
    fn msg(&mut self) -> &[u8];

    /// Default gas host call.
    fn gas(&mut self, amount: u32) -> Result<(), &'static str>;

    /// Charge some extra gas.
    fn charge_gas(&mut self, amount: u32) -> Result<(), &'static str>;

    /// Charge gas by `RuntimeCosts` token.
    fn charge_gas_runtime(&mut self, costs: RuntimeCosts) -> Result<(), &'static str>;

    /// Refund some gas.
    fn refund_gas(&mut self, amount: u32) -> Result<(), &'static str>;

    /// Tell how much gas is left in running context.
    fn gas_available(&mut self) -> Result<u64, &'static str>;

    /// Value associated with message.
    fn value(&mut self) -> Result<u128, &'static str>;

    /// Tell how much value is left in running context.
    fn value_available(&mut self) -> Result<u128, &'static str>;

    /// Interrupt the program and reschedule execution.
    fn wait(&mut self) -> Result<(), &'static str>;

    /// Wake the waiting message and move it to the processing queue.
    fn wake(&mut self, waker_id: MessageId) -> Result<(), &'static str>;

    /// Send init message to create a new program
    fn create_program(&mut self, packet: InitPacket) -> Result<ProgramId, &'static str>;
}

/// Struct for interacting with Ext
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
    /// New ext
    pub fn new(e: E) -> Self {
        Self {
            inner: Rc::new(RefCell::new(Some(e))),
        }
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
        let ext = brw
            .as_mut()
            .ok_or("with should be called only when inner is set")?;

        f(ext)
    }

    /// Unset inner ext
    pub fn take(self) -> Option<E> {
        self.inner.borrow_mut().take()
    }
}

#[cfg(test)]
/// This module contains tests of interacting with LaterExt
mod tests {
    // todo #841 remove most of tests

    use super::*;

    /// Struct with internal value to interact with LaterExt
    #[derive(Debug, PartialEq)]
    struct ExtImplementedStruct(u8);

    /// Empty Ext implementation for test struct
    impl Ext for ExtImplementedStruct {
        fn alloc(
            &mut self,
            _pages: WasmPageNumber,
            _mem: &mut dyn Memory,
        ) -> Result<WasmPageNumber, &'static str> {
            Err("")
        }
        fn block_height(&mut self) -> Result<u32, &'static str> {
            Ok(0)
        }
        fn block_timestamp(&mut self) -> Result<u64, &'static str> {
            Ok(0)
        }
        fn origin(&mut self) -> Result<ProgramId, &'static str> {
            Ok(ProgramId::from(0))
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
            _msg: HandlePacket,
        ) -> Result<MessageId, &'static str> {
            Ok(MessageId::default())
        }
        fn reply_to(&mut self) -> Result<Option<(MessageId, i32)>, &'static str> {
            Ok(None)
        }
        fn source(&mut self) -> Result<ProgramId, &'static str> {
            Ok(ProgramId::from(0))
        }
        fn exit(&mut self, _value_destination: ProgramId) -> Result<(), &'static str> {
            Ok(())
        }
        fn message_id(&mut self) -> Result<MessageId, &'static str> {
            Ok(0.into())
        }
        fn program_id(&mut self) -> Result<ProgramId, &'static str> {
            Ok(0.into())
        }
        fn free(&mut self, _page: WasmPageNumber) -> Result<(), &'static str> {
            Ok(())
        }
        fn debug(&mut self, _data: &str) -> Result<(), &'static str> {
            Ok(())
        }
        fn msg(&mut self) -> &[u8] {
            &[]
        }
        fn gas(&mut self, _amount: u32) -> Result<(), &'static str> {
            Ok(())
        }
        fn charge_gas(&mut self, _amount: u32) -> Result<(), &'static str> {
            Ok(())
        }
        fn charge_gas_runtime(&mut self, _costs: RuntimeCosts) -> Result<(), &'static str> {
            Ok(())
        }
        fn refund_gas(&mut self, _amount: u32) -> Result<(), &'static str> {
            Ok(())
        }
        fn gas_available(&mut self) -> Result<u64, &'static str> {
            Ok(1_000_000)
        }
        fn value(&mut self) -> Result<u128, &'static str> {
            Ok(0)
        }
        fn value_available(&mut self) -> Result<u128, &'static str> {
            Ok(1_000_000)
        }
        fn leave(&mut self) -> Result<(), &'static str> {
            Ok(())
        }
        fn wait(&mut self) -> Result<(), &'static str> {
            Ok(())
        }
        fn wake(&mut self, _waker_id: MessageId) -> Result<(), &'static str> {
            Ok(())
        }
        fn create_program(&mut self, _packet: InitPacket) -> Result<ProgramId, &'static str> {
            Ok(Default::default())
        }
    }

    #[test]
    /// Test that we are able to set and unset LaterExt value
    fn setting_and_unsetting_inner_ext() {
        let ext = LaterExt::new(ExtImplementedStruct(0));

        assert_eq!(
            ext.inner,
            Rc::new(RefCell::new(Some(ExtImplementedStruct(0))))
        );

        let inner = ext.take();

        assert_eq!(inner, Some(ExtImplementedStruct(0)));
    }

    #[test]
    #[allow(clippy::redundant_clone)]
    /// Test that ext's clone still refers to the same inner object as the original one
    fn ext_cloning() {
        let ext_source = LaterExt::new(ExtImplementedStruct(0));
        let ext_clone = ext_source.clone();

        // ext_clone refers the same inner as ext_source,
        let inner = ext_clone.take();

        assert_eq!(inner, Some(ExtImplementedStruct(0)));
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
        let ext = LaterExt::new(ExtImplementedStruct(0));

        let converted_inner = ext.with(converter);

        assert!(converted_inner.is_ok());
    }

    #[test]
    // TODO #841 Change to `should_panic` test
    fn taking_ext_clone() {
        let original_ext = LaterExt::new(ExtImplementedStruct(0));
        let cloned_ext = original_ext.clone();

        assert!(original_ext.take().is_some());
        assert!(cloned_ext.take().is_none());
    }
}
