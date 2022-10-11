use alloc::vec::Vec;
use codec::{Decode, DecodeAll, MaxEncodedLen};
use gear_backend_common::{RuntimeCtx, RuntimeCtxError};
use gear_core::{buffer::RuntimeBuffer, env::Ext, memory::WasmPageNumber};

use gear_core_errors::MemoryError;
use sp_sandbox::{default_executor::Memory as DefaultExecutorMemory, HostError, SandboxMemory};

use crate::{
    funcs::{FuncError, SyscallOutput, WasmCompatible},
    MemoryWrap,
};

pub(crate) struct Runtime<E: Ext> {
    pub ext: E,
    pub memory: DefaultExecutorMemory,
    pub memory_wrap: MemoryWrap,
    pub err: FuncError<E::Error>,
}

impl<E: Ext> Runtime<E> {
    pub(crate) fn run<T, F>(&mut self, f: F) -> SyscallOutput
    where
        T: WasmCompatible,
        F: FnOnce(&mut Self) -> Result<T, FuncError<E::Error>>,
    {
        f(self).map(WasmCompatible::throw_back).map_err(|err| {
            self.err = err;
            HostError
        })
    }
}

impl<E: Ext> RuntimeCtx<E> for Runtime<E> {
    fn alloc(&mut self, pages: u32) -> Result<WasmPageNumber, RuntimeCtxError<E::Error>> {
        self.ext
            .alloc(pages.into(), &mut self.memory_wrap)
            .map_err(RuntimeCtxError::Ext)
    }

    fn read_memory(&self, ptr: u32, len: u32) -> Result<Vec<u8>, RuntimeCtxError<E::Error>> {
        let mut buf = RuntimeBuffer::try_new_default(len as usize)?;
        self.memory
            .get(ptr, buf.get_mut())
            .map_err(|_| MemoryError::OutOfBounds)?;
        Ok(buf.into_vec())
    }

    fn read_memory_into_buf(
        &self,
        ptr: u32,
        buf: &mut [u8],
    ) -> Result<(), RuntimeCtxError<E::Error>> {
        self.memory
            .get(ptr, buf)
            .map_err(|_| MemoryError::OutOfBounds)?;

        Ok(())
    }

    fn read_memory_as<D: Decode + MaxEncodedLen>(
        &self,
        ptr: u32,
    ) -> Result<D, RuntimeCtxError<E::Error>> {
        let buf = self.read_memory(ptr, D::max_encoded_len() as u32)?;
        let decoded = D::decode_all(&mut &buf[..]).map_err(|_| MemoryError::MemoryAccessError)?;
        Ok(decoded)
    }

    fn write_output(&mut self, out_ptr: u32, buf: &[u8]) -> Result<(), RuntimeCtxError<E::Error>> {
        self.memory
            .set(out_ptr, buf)
            .map_err(|_| MemoryError::OutOfBounds)?;

        Ok(())
    }
}
