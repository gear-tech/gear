//! Environment for running a module.

use alloc::rc::Rc;
use core::cell::RefCell;

use codec::{Decode, Encode};

use crate::memory::PageNumber;
use crate::message::OutgoingMessage;
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

    /// Send message to another program.
    fn send(&mut self, msg: OutgoingMessage) -> Result<(), &'static str>;

    /// Get the source of the message currently being handled.
    fn source(&mut self) -> ProgramId;

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

    /// Value associated with message
    fn value(&mut self) -> u128;
}

/// Struct for interacting with Ext
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
