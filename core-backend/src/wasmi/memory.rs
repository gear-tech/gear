//! Wasmi extensions for memory and memory context.

use alloc::boxed::Box;
use core::any::Any;

use gear_core::memory::{Error, Memory, PageNumber};

/// Wrapper for wasmi::MemoryRef.
pub struct MemoryWrap(wasmi::MemoryRef);

impl MemoryWrap {
    /// Wrap wasmi::MemoryRef for Memory trait.
    pub fn new(mem: wasmi::MemoryRef) -> Self {
        MemoryWrap(mem)
    }
}

/// Memory interface for the allocator.
impl Memory for MemoryWrap {
    fn grow(&self, pages: PageNumber) -> Result<PageNumber, Error> {
        self.0
            .grow(wasmi::memory_units::Pages(pages.raw() as usize))
            .map(|prev| (prev.0 as u32).into())
            .map_err(|_| Error::OutOfMemory)
    }

    fn size(&self) -> PageNumber {
        (self.0.current_size().0 as u32).into()
    }

    fn write(&self, offset: usize, buffer: &[u8]) -> Result<(), Error> {
        self.0
            .set(offset as u32, buffer)
            .map_err(|_| Error::MemoryAccessError)
    }

    fn read(&self, offset: usize, buffer: &mut [u8]) {
        self.0
            .get_into(offset as u32, buffer)
            .expect("Memory out of bounds.");
    }

    fn data_size(&self) -> usize {
        (self.0.current_size().0 as u32 * 65536) as usize
    }

    fn data_ptr(&self) -> *mut u8 {
        self.0.direct_access_mut().as_mut().as_mut_ptr()
    }

    fn clone(&self) -> Box<dyn Memory> {
        Box::new(Clone::clone(self))
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
    use gear_core::memory::MemoryContext;

    fn new_test_memory(static_pages: u32, max_pages: u32) -> MemoryContext {
        use core::convert::TryInto;
        use wasmi::{memory_units::Pages, MemoryInstance as WasmMemory};

        let memory = MemoryWrap::new(
            WasmMemory::alloc(
                Pages(static_pages.try_into().unwrap()),
                Some(Pages(max_pages.try_into().unwrap())),
            )
            .expect("Memory creation failed"),
        );

        MemoryContext::new(
            0.into(),
            Box::new(memory),
            Default::default(),
            static_pages.into(),
            max_pages.into(),
        )
    }

    #[test]
    fn smoky() {
        let mut mem = new_test_memory(16, 256);

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
