//! wasmtime function context

use crate::{env::StoreData, memory};
use gear_core::{
    env::{Ext, FunctionContext},
    memory::Error,
};
use wasmtime::{AsContextMut, Caller};

pub struct Context<'c, E: Ext> {
    /// WASM function caller.
    pub caller: Caller<'c, StoreData<E>>,
}

impl<'w, E: Ext> FunctionContext<E> for Context<'w, E> {
    type Error = Error;
    type Memory = wasmtime::Memory;

    fn ext(&self) -> &E {
        &self.caller.data().ext
    }

    fn ext_mut(&mut self) -> &mut E {
        &mut self.caller.data_mut().ext
    }

    fn read_memory_into(
        &mut self,
        mem: Self::Memory,
        offset: usize,
        buffer: &mut [u8],
    ) -> Result<(), Self::Error> {
        memory::read(self.caller.as_context_mut(), mem, offset, buffer)
    }

    fn write_into_memory(
        &mut self,
        mem: Self::Memory,
        offset: usize,
        buffer: &[u8],
    ) -> Result<(), Self::Error> {
        memory::write(self.caller.as_context_mut(), mem, offset, buffer)
    }
}
