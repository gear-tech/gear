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

use crate::memory::{Memory, WasmPageNumber};
use crate::{
    ids::{MessageId, ProgramId},
    message::{ExitCode, HandlePacket, InitPacket, ReplyPacket},
};
use alloc::rc::Rc;
use anyhow::Result;
use codec::{Decode, Encode};
use core::cell::RefCell;
use core::ops::Deref;

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
    fn block_height(&self) -> u32;

    /// Get the current block timestamp.
    fn block_timestamp(&self) -> u64;

    /// Get the id of the user who initiated communication with blockchain,
    /// during which, currently processing message was created.
    fn origin(&self) -> ProgramId;

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
    fn reply_to(&self) -> Option<(MessageId, ExitCode)>;

    /// Get the source of the message currently being handled.
    fn source(&mut self) -> ProgramId;

    /// Terminate the program and transfer all available value to the address.
    fn exit(&mut self, value_destination: ProgramId) -> Result<(), &'static str>;

    /// Get the id of the message currently being handled.
    fn message_id(&mut self) -> MessageId;

    /// Get the id of program itself
    fn program_id(&self) -> ProgramId;

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

    /// Charge some gas.
    fn charge_gas(&mut self, amount: u32) -> Result<(), &'static str>;

    /// Refund some gas.
    fn refund_gas(&mut self, amount: u32) -> Result<(), &'static str>;

    /// Tell how much gas is left in running context.
    fn gas_available(&self) -> u64;

    /// Value associated with message.
    fn value(&self) -> u128;

    /// Tell how much value is left in running context.
    fn value_available(&self) -> u128;

    /// Interrupt the program and reschedule execution.
    fn wait(&mut self) -> Result<(), &'static str>;

    /// Wake the waiting message and move it to the processing queue.
    fn wake(&mut self, waker_id: MessageId) -> Result<(), &'static str>;

    /// Send init message to create a new program
    fn create_program(&mut self, packet: InitPacket) -> Result<ProgramId, &'static str>;
}

/// Basic struct for interacting with external API, provided to the program.
/// 
/// Stores `Ext` type and manages it with reference counter. RC is used to make the carrier
/// optionally clone-able.
/// 
/// The struct can't be instantiated outside the module although it has `pub` visibility. 
/// This is done intentionally in order to provide dereference to it for wrappers which 
/// are used outside the module ([`ExtCarrier`], [`ReplicableExtCarrier`]) and to reduce
/// repetitiveness of `with`/`with_fallible` methods.  
// TODO #852 type will be redundant after resolving the issue.
pub struct BaseExtCarrier<E: Ext> {
    inner: Rc<RefCell<Option<E>>>,
}

impl<E: Ext> BaseExtCarrier<E> {
    // New base ext carrier
    fn new(e: E) -> Self {
        Self {
            inner: Rc::new(RefCell::new(Some(e))),
        }
    }

    /// Calls infallible fn with inner ext
    pub fn with<R>(&self, f: impl FnOnce(&mut E) -> R) -> Result<R, &'static str> {
        self.with_fallible(|e| Ok(f(e)))
    }

    /// Calls fallible fn with inner ext
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
}

/// Struct for interacting with Ext.
/// 
/// Unlike [`BaseExtCarrier`] this struct is intended for external usage.
pub struct ExtCarrier<E: Ext>(BaseExtCarrier<E>); // todo [sab] remove Rc

impl<E: Ext> ExtCarrier<E> {
    /// New ext carrier
    pub fn new(e: E) -> Self {
        Self(BaseExtCarrier::new(e))
    }

    /// Take ownership over wrapped ext.
    pub fn take(self) -> E {
        let BaseExtCarrier { inner } = self.0;
        inner
            .take()
            .expect("can be called only once during instance consumption; qed")
    }
}

impl<E: Ext> Deref for ExtCarrier<E> {
    type Target = BaseExtCarrier<E>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// "Clone-able" struct for interacting with Ext.
/// 
/// Unlike [`BaseExtCarrier`] this struct is intended for external usage.
// TODO #852 type will be redundant after resolving the issue.
pub struct ReplicableExtCarrier<E: Ext>(BaseExtCarrier<E>);

impl<E: Ext> ReplicableExtCarrier<E> {
    /// New clone-able ext carrier
    pub fn new(e: E) -> Self {
        Self(BaseExtCarrier::new(e))
    }

    /// Take ownership over wrapped ext.
    /// 
    /// Because of the fact that the type is actually a wrapper over `Rc`, 
    /// we have no guarantee that `take` can be called only once on the same
    /// data. That's why `Option<E>` is returned instead of `E` as in [`ExtCarrier`]
    pub fn take(self) -> Option<E> {
        self.0.inner.take()
    }
}

impl<E: Ext> Deref for ReplicableExtCarrier<E> {
    type Target = BaseExtCarrier<E>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<E: Ext> Clone for ReplicableExtCarrier<E> {
    fn clone(&self) -> Self {
        let BaseExtCarrier { inner } = &self.0;
        Self(BaseExtCarrier {
            inner: inner.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test function of format `Fn(&mut E: Ext) -> R`
    // to call `fn with<R>(&self, f: impl FnOnce(&mut E) -> R) -> R`.
    // For example, returns the field of ext's inner value.
    fn converter(e: &mut ExtImplementedStruct) -> u8 {
        e.0
    }

    /// Struct with internal value to interact with ExtCarrier
    #[derive(Debug, PartialEq, Clone, Copy)]
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
        fn block_height(&self) -> u32 {
            0
        }
        fn block_timestamp(&self) -> u64 {
            0
        }
        fn origin(&self) -> ProgramId {
            ProgramId::from(0)
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
        fn reply_to(&self) -> Option<(MessageId, ExitCode)> {
            None
        }
        fn source(&mut self) -> ProgramId {
            ProgramId::from(0)
        }
        fn exit(&mut self, _value_destination: ProgramId) -> Result<(), &'static str> {
            Ok(())
        }
        fn message_id(&mut self) -> MessageId {
            0.into()
        }
        fn program_id(&self) -> ProgramId {
            0.into()
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
        fn charge_gas(&mut self, _amount: u32) -> Result<(), &'static str> {
            Ok(())
        }
        fn refund_gas(&mut self, _amount: u32) -> Result<(), &'static str> {
            Ok(())
        }
        fn gas_available(&self) -> u64 {
            1_000_000
        }
        fn value(&self) -> u128 {
            0
        }
        fn value_available(&self) -> u128 {
            1_000_000
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
    /// Test that we are able to instantiate and take ExtCarrier value
    fn create_take_ext_carrier() {
        let ext_implementer = ExtImplementedStruct(0);
        let ext = ExtCarrier::new(ext_implementer);

        assert_eq!(
            ext.0.inner,
            Rc::new(RefCell::new(Some(ext_implementer)))
        );

        let inner = ext.take();

        assert_eq!(inner, ext_implementer);
    }

    #[test]
    /// Test that ext's `with<R>(...)` works correct when the inner is set
    fn calling_fn_with_inner_ext() {
        let ext_implementer = ExtImplementedStruct(0);
        let ext = ExtCarrier::new(ext_implementer);
        let r_ext = ReplicableExtCarrier::new(ext_implementer);

        assert!(ext.with(converter).is_ok());
        assert!(r_ext.with(converter).is_ok());        
    }

    #[test]
    #[allow(clippy::redundant_clone)]
    /// Test that ext's clone still refers to the same inner object as the original one
    fn ext_cloning() {
        let ext_implementer = ExtImplementedStruct(0);
        let ext = ReplicableExtCarrier::new(ext_implementer);
        let ext_clone = ext.clone();

        let inner = ext_clone.take().expect("ext is set");

        assert_eq!(inner, ext_implementer);
    }

    #[test]
    fn taking_ext_with_clone() {
        let ext = ReplicableExtCarrier::new(ExtImplementedStruct(0));
        let ext_clone = ext.clone();

        assert!(ext_clone.take().is_some());
        assert!(ext.take().is_none())
    }

    #[test]
    /// Test that calling ext's `with<R>(...)` throws error
    /// when the inner value was taken
    fn calling_fn_with_empty_ext() {
        let ext = ReplicableExtCarrier::new(ExtImplementedStruct(0));
        let ext_clone = ext.clone();

        let _ = ext.take();
        assert_eq!(
            ext_clone.with(converter).unwrap_err(),
            "with should be called only when inner is set"
        );
    }
}
