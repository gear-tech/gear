use crate::program::ProgramId;
use crate::storage::AllocationStorage;
use codec::{Decode, Encode};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Debug)]
pub enum Error {
    OutOfMemory,
    AllocationsInUse,
    PageOccupied(PageNumber),
    InvalidFree(PageNumber),
}

#[derive(
    Clone, Copy, Debug, Decode, Encode, derive_more::From, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct PageNumber(u32);

impl PageNumber {
    pub fn raw(&self) -> u32 {
        self.0
    }
}

pub trait Memory {
    fn grow(&self, pages: PageNumber) -> Result<PageNumber, Error>;
    fn size(&self) -> PageNumber;
    unsafe fn data_unchecked(&self) -> &[u8];
    // unsafe fn data_unchecked_mut(&mut self) -> &mut [u8];
    fn clone(&self) -> Box<dyn Memory>;
    fn read(&self, offset: usize, buffer: &mut [u8]) -> Result<(), Error>;
    fn write(&self, offset: usize, buffer: &[u8]) -> Result<(), Error>;
}

impl Memory for wasmtime::Memory {
    fn grow(&self, pages: PageNumber) -> Result<PageNumber, Error> {
        self.grow(pages.raw())
            .map(Into::into)
            .map_err(|_| Error::OutOfMemory)
    }

    fn size(&self) -> PageNumber {
        self.size().into()
    }

    unsafe fn data_unchecked(&self) -> &[u8] {
        self.data_unchecked()
    }

    unsafe fn data_unchecked_mut(&self) -> &mut [u8] {
        self.data_unchecked_mut()
    }

    fn clone(&self) -> Box<dyn Memory> {
        Box::new(Clone::clone(self))
    }

    fn read(&self, offset: usize, buffer: &mut [u8]) -> Result<(), Error> {
        if self.read(offset, buffer).is_err() {
            return Err(Error::OutOfMemory);
        }
        Ok(())
    }

    fn write(&self, offset: usize, buffer: &[u8]) -> Result<(), Error> {
        if self.write(offset, buffer).is_err() {
            return Err(Error::OutOfMemory);
        }
        Ok(())
    }
}

pub struct Allocations<AS: AllocationStorage>(Rc<RefCell<AS>>);

impl<AS: AllocationStorage> Clone for Allocations<AS> {
    fn clone(&self) -> Self {
        Allocations(self.0.clone())
    }
}

impl<AS: AllocationStorage> Allocations<AS> {
    pub fn new(storage: AS) -> Self {
        Self(Rc::new(RefCell::new(storage)))
    }

    pub fn get(&self, page: PageNumber) -> Option<ProgramId> {
        self.0.borrow().get(page).copied()
    }

    pub fn occupied(&self, page: PageNumber) -> bool {
        self.0.borrow().exists(page)
    }

    pub fn insert(&self, program_id: ProgramId, page: PageNumber) -> Result<(), Error> {
        if self.0.borrow().exists(page) {
            return Err(Error::PageOccupied(page));
        }

        self.0.borrow_mut().set(page, program_id);

        Ok(())
    }

    pub fn remove(&self, program_id: ProgramId, page: PageNumber) -> Result<(), Error> {
        if program_id != *self.0.borrow().get(page).ok_or(Error::InvalidFree(page))? {
            return Err(Error::InvalidFree(page));
        }

        self.0.borrow_mut().remove(page);

        Ok(())
    }

    pub fn drain(self) -> Result<AS, Error> {
        Ok(Rc::try_unwrap(self.0)
            .map_err(|_| Error::AllocationsInUse)?
            .into_inner())
    }

    pub fn clear(&self, program_id: ProgramId) {
        self.0.borrow_mut().clear(program_id)
    }

    pub fn len(&self) -> usize {
        self.0.borrow().count()
    }
}

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
    pub fn new(
        program_id: ProgramId,
        memory: Box<dyn Memory>,
        allocations: Allocations<AS>,
        static_pages: PageNumber,
        max_pages: PageNumber,
    ) -> Self {
        Self {
            memory,
            program_id,
            allocations,
            static_pages,
            max_pages,
        }
    }

    pub fn alloc(&self, pages: PageNumber) -> Result<PageNumber, Error> {
        // silly allocator, brute-forces fist continuous sector
        let mut candidate = self.static_pages.raw();
        let mut found = 0u32;

        while found < pages.raw() {
            if candidate + pages.raw() > self.max_pages.raw() {
                log::debug!(
                    "candidate: {}, pages: {}, max_pages: {}",
                    candidate,
                    pages.raw(),
                    self.max_pages.raw()
                );
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

        for page_num in candidate..candidate + found {
            self.allocations.insert(self.program_id, page_num.into())?;
        }

        Ok(candidate.into())
    }

    pub fn free(&self, page: PageNumber) -> Result<(), Error> {
        if page < self.static_pages || page > self.max_pages {
            return Err(Error::InvalidFree(page));
        }

        self.allocations.remove(self.program_id, page)?;

        Ok(())
    }

    pub fn allocations(&self) -> &Allocations<AS> {
        &self.allocations
    }

    pub fn memory(&mut self) -> &dyn Memory {
        &mut *self.memory
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::InMemoryAllocationStorage;

    fn new_test_memory(
        static_pages: u32,
        max_pages: u32,
    ) -> MemoryContext<InMemoryAllocationStorage> {
        use wasmtime::{Engine, Limits, Memory as WasmMemory, MemoryType, Store};

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
        assert_eq!(mem.allocations().len(), 0);

        assert_eq!(mem.alloc(16.into()).expect("allocation failed"), 16.into());
        assert_eq!(mem.allocations().len(), 16);

        // there is a space for 14 more
        for _ in 0..14 {
            mem.alloc(16.into()).expect("allocation failed");
        }

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
