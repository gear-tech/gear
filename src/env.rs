//! Environment for running a module.

use alloc::rc::Rc;
use core::cell::RefCell;

use codec::{Decode, Encode};

use crate::memory::PageNumber;
use crate::message::{MessageId, OutgoingPacket, ReplyPacket};
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

/// Page information.
pub type PageInfo = (PageNumber, PageAction, *const u8);

/// External api for managing memory, messages, allocations and gas-counting.
pub trait Ext {
    /// Allocate number of pages.
    ///
    /// The resulting page number should point to `pages` consecutives memory pages.
    fn alloc(&mut self, pages: PageNumber) -> Result<PageNumber, &'static str>;

    /// Send message to another program.
    fn send(&mut self, msg: OutgoingPacket) -> Result<(), &'static str>;

    /// Produce reply to the current message.
    fn reply(&mut self, msg: ReplyPacket) -> Result<(), &'static str>;

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
    fn get_mem(&mut self, ptr: usize, buffer: &mut [u8]);

    /// Access currently handled message payload.
    fn msg(&mut self) -> &[u8];

    /// Query memory access rights to the specific page.
    fn memory_access(&self, page: PageNumber) -> PageAction;

    /// Lock entire memory from any access.
    fn memory_lock(&self);

    /// Unlock entire memory region for any access.
    fn memory_unlock(&self);

    /// Report that some gas has been used.
    fn gas(&mut self, amount: u32) -> Result<(), &'static str>;

    /// Transfer gas to program from the caller side.
    fn charge(&mut self, gas: u64) -> Result<(), &'static str>;

    /// Value associated with message
    fn value(&mut self) -> u128;
}

/// Struct for interacting with Ext
#[derive(Default)]
pub struct LaterExt<E: Ext> {
    inner: Rc<RefCell<Option<E>>>,
}

impl<E: Ext> Clone for LaterExt<E> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
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
    pub fn with<R>(&self, f: impl FnOnce(&mut E) -> R) -> R {
        let mut brw = self.inner.borrow_mut();
        let mut ext = brw
            .take()
            .expect("with should be called only when inner is set");
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
        fn send(&mut self, _msg: OutgoingPacket) -> Result<(), &'static str> {
            Ok(())
        }
        fn reply(&mut self, _msg: ReplyPacket) -> Result<(), &'static str> {
            Ok(())
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
        fn get_mem(&mut self, _ptr: usize, _buffer: &mut [u8]) {}
        fn msg(&mut self) -> &[u8] {
            &[]
        }
        fn memory_access(&self, _page: PageNumber) -> PageAction {
            PageAction::None
        }
        fn memory_lock(&self) {}
        fn memory_unlock(&self) {}
        fn gas(&mut self, _amount: u32) -> Result<(), &'static str> {
            Ok(())
        }
        fn value(&mut self) -> u128 {
            0
        }
        fn charge(&mut self, _gas: u64) -> Result<(), &'static str> {
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

        assert_eq!(converted_inner, 0);
    }

    #[test]
    #[should_panic(expected = "with should be called only when inner is set")]
    /// Test that calling ext's `with<R>(...)` causes panic
    /// when the inner value was not set or was unsetted
    fn calling_fn_with_empty_ext() {
        let ext = LaterExt::<ExtImplementedStruct>::new();

        let _ = ext.with(converter);
    }
}
