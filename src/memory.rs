//! Module for memory and memory context.

use codec::{Encode, Decode};
use std::{rc::Rc, cell::RefCell};
use crate::program::ProgramId;
use crate::storage::AllocationStorage;

/// Memory error.
#[derive(Clone, Debug)]
pub enum Error {
    /// Memory is over.
    ///
    /// All pages were previously allocated and there is nothing can be done.
    OutOfMemory,

    /// Allocation is in use.
    ///
    /// This is probably mis-use of the api (like dropping `Allocations` struct when some code is still runnig).
    AllocationsInUse,

    /// Specified page is occupied.
    PageOccupied(PageNumber),

    /// Specified page cannot be freed by the current program.
    ///
    /// It was allocated by another program.
    InvalidFree(PageNumber),
    
    /// Out of bounds memory access
    MemoryAccessError,
}

/// Page number.
#[derive(Clone, Copy, Debug, Decode, Encode, derive_more::From, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageNumber(u32);

impl PageNumber {
    /// Return raw 32-bit page address.
    pub fn raw(&self) -> u32 { self.0 }
}

impl std::ops::Add for PageNumber {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl std::ops::Sub for PageNumber {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

/// Memory interface for the allocator.
pub trait Memory {
    /// Grow memory by number of pages.
    fn grow(&self, pages: PageNumber) -> Result<PageNumber, Error>;

    /// Return current size of the memory.
    fn size(&self) -> PageNumber;

    /// Set memory region at specific pointer.
    fn write(&self, offset: usize, buffer: &[u8]) -> Result<(), Error>;

    /// Reads memory contents at the given offset into a buffer.
    fn read(&self, offset: usize, buffer: &mut [u8]);

    /// Cloen this memory.
    fn clone(&self) -> Box<dyn Memory>;

    /// Lock some memory pages.
    fn lock(&self, offset: PageNumber, length: PageNumber) -> *mut u8;

    /// Unlock some memory pages.
    fn unlock(&self, offset: PageNumber, length: PageNumber);
}

impl Memory for wasmtime::Memory {
    fn grow(&self, pages: PageNumber) -> Result<PageNumber, Error> {
        self.grow(pages.raw())
            .map(|offset| {
                // lock pages after grow
                self.lock(offset.into(), pages);
                offset.into()
            })
            .map_err(|_| Error::OutOfMemory)
    }

    fn size(&self) -> PageNumber {
        self.size().into()
    }

    fn write(&self, offset: usize, buffer: &[u8]) -> Result<(), Error> {
        self.write(offset, buffer).or_else(|_| Err(Error::MemoryAccessError))
    }

    fn read(&self, offset: usize, buffer: &mut [u8]) {
        self.read(offset, buffer).expect("Memory out of bounds.");
    }

    fn clone(&self) -> Box<dyn Memory> {
        Box::new(Clone::clone(self))
    }

    fn lock(&self, offset: PageNumber, length: PageNumber) -> *mut u8 {
        let base = self.data_ptr().wrapping_add(65536 * offset.raw() as usize);
        let length = 65536usize * length.raw() as usize;

        // So we can later trigger SIGSEGV by performing a read
        unsafe {
            libc::mprotect(base as *mut libc::c_void, length, libc::PROT_NONE);
        }
        base
    }

    fn unlock(&self, offset: PageNumber, length: PageNumber) {
        let base = self.data_ptr().wrapping_add(65536 * offset.raw() as usize);
        let length = 65536usize * length.raw() as usize;

        // Set r/w protection
        unsafe {
            libc::mprotect(
                base as *mut libc::c_void,
                length,
                libc::PROT_READ | libc::PROT_WRITE,
            );
        }
    }
}

/// Helper struct to manage allocations requested by programs.
///
/// Underlying allocation storage can be anything.
pub struct Allocations<AS: AllocationStorage>(Rc<RefCell<AS>>);

impl<AS: AllocationStorage> Clone for Allocations<AS> {
    fn clone(&self) -> Self {
        Allocations(self.0.clone())
    }
}

impl<AS: AllocationStorage> Allocations<AS> {
    /// New allocation maanager.
    pub fn new(storage: AS) -> Self {
        Self(Rc::new(RefCell::new(storage)))
    }

    /// Get page owner, if any.
    pub fn get(&self, page: PageNumber) -> Option<ProgramId> {
        self.0.borrow().get(page)
    }

    /// Check if specific page is allocated by anything.
    pub fn occupied(&self, page: PageNumber) -> bool {
        self.0.borrow().exists(page)
    }

    /// Insert new allocation.
    pub fn insert(&self, program_id: ProgramId, page: PageNumber) -> Result<(), Error> {
        if self.0.borrow().exists(page) {
            return Err(Error::PageOccupied(page))
        }

        self.0.borrow_mut().set(page, program_id);

        Ok(())
    }

    /// Remove specific allocation.
    ///
    /// Owner and provided `program_id` must match.
    pub fn remove(&self, program_id: ProgramId, page: PageNumber) -> Result<(), Error> {
        if program_id != self.0.borrow().get(page).ok_or(Error::InvalidFree(page))? {
            return Err(Error::InvalidFree(page));
        }

        self.0.borrow_mut().remove(page);

        Ok(())
    }

    /// Drop allocation manager and return underlying `AllocationStorage`
    pub fn drain(self) -> Result<AS, Error> {
        Ok(Rc::try_unwrap(self.0).map_err(|_| Error::AllocationsInUse)?.into_inner())
    }

}

/// Memory context for the running program.
pub struct MemoryContext<AS: AllocationStorage> {
    program_id: ProgramId,
    memory: Box<dyn Memory>,
    allocations: Allocations<AS>,
    max_pages: PageNumber,
    static_pages: PageNumber,
}

impl<AS: AllocationStorage> Clone for MemoryContext<AS> {
    fn clone(&self) -> Self {
        Self {
            program_id: self.program_id,
            memory: self.memory.clone(),
            allocations: self.allocations.clone(),
            max_pages: self.max_pages,
            static_pages: self.static_pages,
        }
    }
}

impl Clone for Box<dyn Memory> {
    fn clone(self: &Box<dyn Memory>) -> Box<dyn Memory> {
        Memory::clone(&**self)
    }
}

impl<AS: AllocationStorage> MemoryContext<AS> {
    /// New memory context.
    ///
    /// Provide currently running `program_id`, boxed memory abstraction
    /// and allocation manager. Also configurable `static_pages` and `max_pages`
    /// are set.
    pub fn new(
        program_id: ProgramId,
        memory: Box<dyn Memory>,
        allocations: Allocations<AS>,
        static_pages: PageNumber,
        max_pages: PageNumber,
    ) -> Self {
        Self { memory, program_id, allocations, static_pages, max_pages }
    }

    /// Return currently used program id.
    pub fn program_id(&self) -> ProgramId {
        self.program_id
    }

    /// Alloc specific number of pages for the currently running program.
    pub fn alloc(&self, pages: PageNumber) -> Result<PageNumber, Error> {
        // silly allocator, brute-forces fist continuous sector
        let mut candidate = self.static_pages.raw();
        let mut found = 0u32;

        while found < pages.raw() {
            if candidate + pages.raw() > self.max_pages.raw() {
                log::debug!("candidate: {}, pages: {}, max_pages: {}", candidate, pages.raw(), self.max_pages.raw());
                return Err(Error::OutOfMemory);
            }

            if self.allocations.occupied((candidate + found).into()) {
                candidate += 1;
                found = 0;
                continue;
            }

            found += 1;
        }

        if candidate + found > self.memory.size().raw() {
            let extra_grow = candidate + found - self.memory.size().raw();
            self.memory.grow(extra_grow.into())?;
        }

        for page_num in candidate..candidate+found {
            self.allocations.insert(self.program_id, page_num.into())?;
        }

        Ok(candidate.into())
    }

    /// Free specific page.
    ///
    /// Currently running program should own this page.
    pub fn free(&self, page: PageNumber) -> Result<(), Error> {
        if page < self.static_pages || page > self.max_pages {
            return Err(Error::InvalidFree(page));
        }

        self.allocations.remove(self.program_id, page)?;

        Ok(())
    }

    /// Return reference to the allocation manager.
    pub fn allocations(&self) -> &Allocations<AS> {
        &self.allocations
    }

    /// Return reference to the memory blob.
    pub fn memory(&self) -> &dyn Memory {
        &*self.memory
    }

    /// Lock memory access.
    pub fn memory_lock(&self) {
        self.memory.lock(self.static_pages, self.max_pages - self.static_pages);
    }

    /// Unlock memory access.
    pub fn memory_unlock(&self) {
        self.memory.unlock(self.static_pages, self.max_pages - self.static_pages);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::InMemoryAllocationStorage;

    fn new_test_memory(static_pages: u32, max_pages: u32) -> MemoryContext<InMemoryAllocationStorage> {
        use wasmtime::{Engine, Store, MemoryType, Memory as WasmMemory, Limits};

        let engine = Engine::default();
        let store = Store::new(&engine);

        let memory_ty = MemoryType::new(Limits::new(static_pages, Some(max_pages)));
        let memory = WasmMemory::new(&store, memory_ty);

        MemoryContext::new(
            0.into(),
            Box::new(memory),
            Allocations::new(InMemoryAllocationStorage::new(Vec::new())),
            static_pages.into(),
            max_pages.into(),
        )
    }

    #[test]
    fn smoky() {
        let mem = new_test_memory(16, 256);

        assert_eq!(mem.alloc(16.into()).expect("allocation failed"), 16.into());

        // there is a space for 14 more
        for _ in 0..14 { mem.alloc(16.into()).expect("allocation failed"); }

        // no more mem!
        assert!(mem.alloc(1.into()).is_err());

        // but we free some
        mem.free(137.into()).expect("free failed");

        // and now can allocate page that was freed
        assert_eq!(mem.alloc(1.into()).expect("allocation failed").raw(), 137);

        // if we have 2 in a row we can allocate even 2
        mem.free(117.into()).expect("free failed");
        mem.free(118.into()).expect("free failed");

        assert_eq!(mem.alloc(2.into()).expect("allocation failed").raw(), 117);

        // but if 2 are not in a row, bad luck
        mem.free(117.into()).expect("free failed");
        mem.free(158.into()).expect("free failed");

        assert!(mem.alloc(2.into()).is_err());
    }
}
