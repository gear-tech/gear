use std::{rc::Rc, cell::{RefCell, Ref}, collections::HashSet};
use super::PageNumber;

#[derive(Clone, Debug)]
pub enum Error {
    OutOfMemory,
    InvalidFree(PageNumber),
}

pub struct Memory {
    wasm: wasmtime::Memory,
    allocations: Rc<RefCell<HashSet<PageNumber>>>,
    max_pages: PageNumber,
    static_pages: PageNumber,
}

impl Memory {
    pub fn new(
        wasm_memory: wasmtime::Memory,
        allocations: Rc<RefCell<HashSet<PageNumber>>>,
        static_pages: PageNumber,
        max_pages: PageNumber,
    ) -> Self {
        Self { wasm: wasm_memory, allocations, static_pages, max_pages }
    }

    pub fn alloc(&self, pages: PageNumber) -> Result<PageNumber, Error> {
        // silly allocator, brute-forces fist continuous sector
        let mut candidate = self.static_pages.raw();
        let mut found = 0u32;
        let mut allocations = self.allocations.borrow_mut();

        while found < pages.raw() {
            if candidate + pages.raw() > self.max_pages.raw() {
                println!("candidate: {}, pages: {}, max_pages: {}", candidate, pages.raw(), self.max_pages.raw());
                return Err(Error::OutOfMemory);
            }

            if allocations.contains(&(candidate + found).into()) {
                candidate += 1;
                found = 0;
                continue;
            }

            found += 1;
        }

        if candidate + found > self.wasm.size() {
            let extra_grow = candidate + found - self.wasm.size();
            self.wasm.grow(extra_grow).map_err(|_grow_err| Error::OutOfMemory)?;
        }

        for page_num in candidate..candidate+found {
            allocations.insert(page_num.into());
        }

        Ok(candidate.into())
    }

    pub fn free(&self, page: PageNumber) -> Result<(), Error> {
        if page < self.static_pages || page > self.max_pages {
            return Err(Error::InvalidFree(page));
        }

        if !self.allocations.borrow_mut().remove(&page) {
            return Err(Error::InvalidFree(page));
        }

        Ok(())
    }

    pub fn allocations(&self) -> Ref<'_, HashSet<PageNumber>> {
        self.allocations.borrow()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_test_memory(static_pages: u32, max_pages: u32) -> Memory {
        use wasmtime::{Engine, Store, MemoryType, Memory as WasmMemory, Limits};

        let engine = Engine::default();
        let store = Store::new(&engine);

        let memory_ty = MemoryType::new(Limits::new(static_pages, Some(max_pages)));
        let memory = WasmMemory::new(&store, memory_ty);

        Memory::new(memory, Rc::new(RefCell::new(HashSet::new())), static_pages.into(), max_pages.into())
    }

    #[test]
    fn smoky() {
        let mem = new_test_memory(16, 256);
        assert_eq!(mem.allocations().len(), 0);

        assert_eq!(mem.alloc(16.into()).expect("allocation failed"), 16.into());
        assert_eq!(mem.allocations().len(), 16);

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
