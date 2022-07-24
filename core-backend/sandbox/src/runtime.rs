use alloc::{vec, vec::Vec};
use codec::{Decode, DecodeAll, MaxEncodedLen};
use gear_core::env::Ext;

use gear_core_errors::MemoryError;
use sp_sandbox::{default_executor::Memory as DefaultExecutorMemory, SandboxMemory};

use crate::{funcs::FuncError, MemoryWrap};

pub(crate) struct Runtime<'a, E: Ext> {
    pub ext: &'a mut E,
    pub memory: &'a DefaultExecutorMemory,
    pub memory_wrap: &'a mut MemoryWrap,
    pub err: FuncError<E::Error>,
}

impl<'a, E> Runtime<'a, E>
where
    E: Ext + 'a,
{
    /// Allocate new pages in module memory.
    pub fn alloc(
        &mut self,
        pages: u32,
    ) -> Result<gear_core::memory::WasmPageNumber, <E as Ext>::Error> {
        self.ext.alloc(pages.into(), self.memory_wrap)
    }

    /// Read designated chunk from the sandbox memory.
    ///
    /// Returns `Err` if one of the following conditions occurs:
    ///
    /// - requested buffer is not within the bounds of the sandbox memory.
    pub fn read_memory(&self, ptr: u32, len: u32) -> Result<Vec<u8>, MemoryError> {
        let mut buf = vec![0u8; len as usize];
        self.memory
            .get(ptr, buf.as_mut_slice())
            .map_err(|_| MemoryError::OutOfBounds)?;
        Ok(buf)
    }

    /// Read designated chunk from the sandbox memory into the supplied buffer.
    ///
    /// Returns `Err` if one of the following conditions occurs:
    ///
    /// - requested buffer is not within the bounds of the sandbox memory.
    pub fn read_memory_into_buf(
        &self,
        ptr: u32,
        buf: &mut [u8],
    ) -> Result<(), MemoryError> {
        self.memory
            .get(ptr, buf)
            .map_err(|_| MemoryError::OutOfBounds)
    }

    /// Reads and decodes a type with a size fixed at compile time from contract memory.
    pub fn read_memory_as<D: Decode + MaxEncodedLen>(
        &self,
        ptr: u32,
    ) -> Result<D, MemoryError> {
        let buf = self.read_memory(ptr, D::max_encoded_len() as u32)?;
        let decoded = D::decode_all(&mut &buf[..]).map_err(|_| MemoryError::MemoryAccessError)?;
        Ok(decoded)
    }

    /// Write the given buffer and its length to the designated locations in sandbox memory.
    //
    /// `out_ptr` is the location in sandbox memory where `buf` should be written to.
    pub fn write_output(&mut self, out_ptr: u32, buf: &[u8]) -> Result<(), MemoryError> {
        self.memory
            .set(out_ptr, buf)
            .map_err(|_| MemoryError::OutOfBounds)?;

        Ok(())
    }
}
