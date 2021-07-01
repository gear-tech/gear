//! Wasmtime extensions for memory and memory context.

use alloc::boxed::Box;
use core::any::Any;

use gear_core::memory::{Error, PageNumber, Memory};

/// Wrapper for wasmtime memory.
pub struct MemoryWrap(wasmtime::Memory);

impl MemoryWrap {
    /// Wrap wasmtime memory for Memory trait.
    pub fn new(mem: wasmtime::Memory) -> Self {
        MemoryWrap(mem)
    }
}

/// Memory interface for the allocator.
impl Memory for MemoryWrap {
    fn grow(&self, pages: PageNumber) -> Result<PageNumber, Error> {
        self.0
            .grow(pages.raw())
            .map(|offset| {
                cfg_if::cfg_if! {
                    if #[cfg(target_os = "linux")] {

                        // lock pages after grow
                        self.lock(offset.into(), pages);
                    }
                }
                offset.into()
            })
            .map_err(|_| Error::OutOfMemory)
    }

    fn size(&self) -> PageNumber {
        self.0.size().into()
    }

    fn write(&self, offset: usize, buffer: &[u8]) -> Result<(), Error> {
        self.0
            .write(offset, buffer)
            .map_err(|_| Error::MemoryAccessError)
    }

    fn read(&self, offset: usize, buffer: &mut [u8]) {
        self.0.read(offset, buffer).expect("Memory out of bounds.");
    }

    fn data_size(&self) -> usize {
        self.0.data_size()
    }

    fn data_ptr(&self) -> *mut u8 {
        self.0.data_ptr()
    }

    fn clone(&self) -> Box<dyn Memory> {
        Box::new(Clone::clone(self))
    }

    fn lock(&self, offset: PageNumber, length: PageNumber) -> *mut u8 {
        let base = self
            .0
            .data_ptr()
            .wrapping_add(65536 * offset.raw() as usize);
        let length = 65536usize * length.raw() as usize;

        // So we can later trigger SIGSEGV by performing a read
        unsafe {
            libc::mprotect(base as *mut libc::c_void, length, libc::PROT_NONE);
        }
        base
    }

    fn unlock(&self, offset: PageNumber, length: PageNumber) {
        let base = self
            .0
            .data_ptr()
            .wrapping_add(65536 * offset.raw() as usize);
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

    fn as_any(&self) -> &dyn Any {
        &self.0
    }
}

impl Clone for MemoryWrap {
    fn clone(self: &MemoryWrap) -> Self {
        MemoryWrap(self.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use gear_core::storage::InMemoryAllocationStorage;
    use gear_core::memory::{Allocations, MemoryContext};

    fn new_test_memory(
        static_pages: u32,
        max_pages: u32,
    ) -> MemoryContext<InMemoryAllocationStorage> {
        use wasmtime::{Engine, Limits, Memory as WasmMemory, MemoryType, Store};

        let engine = Engine::default();
        let store = Store::new(&engine);

        let memory_ty = MemoryType::new(Limits::new(static_pages, Some(max_pages)));
        let memory = MemoryWrap::new(WasmMemory::new(&store, memory_ty).expect("Memory creation failed"));

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
