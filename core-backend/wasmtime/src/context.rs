//! wasmtime function context

use crate::{
    env::StoreData,
    funcs::FuncError,
    memory::{self, MemoryWrap},
};
use alloc::{rc::Rc, vec, vec::Vec};
use core::cell::RefCell;
use gear_backend_common::TerminationReason;
use gear_core::{
    env::{Ext, FunctionContext},
    memory::{Error, Memory},
};
use gear_core_errors::MemoryError;
use wasmtime::{AsContextMut, Caller, StoreContextMut, Trap};

pub struct Context<'c, E: Ext> {
    /// WASM function caller.
    pub caller: Caller<'c, StoreData<E>>,
}

impl<'c, E: Ext> Context<'c, E> {
    pub fn rc(mut self) -> Rc<RefCell<Caller<'c, StoreData<E>>>> {
        Rc::new(RefCell::new(self.caller))
    }

    pub fn get_bytes32(&mut self, mem: &wasmtime::Memory, ptr: usize) -> Result<[u8; 32], Trap> {
        let mut ret = [0u8; 32];
        self.read_memory_into(mem, ptr, &mut ret)?;
        Ok(ret)
    }

    pub fn get_u128(&mut self, mem: &wasmtime::Memory, ptr: usize) -> Result<u128, Trap> {
        let mut u128_le = [0u8; 16];
        self.read_memory_into(mem, ptr, &mut u128_le)?;
        Ok(u128::from_le_bytes(u128_le))
    }

    pub fn get_vec(
        &mut self,
        mem: &wasmtime::Memory,
        ptr: usize,
        len: usize,
    ) -> Result<Vec<u8>, Trap> {
        let mut vec = vec![0u8; len];
        self.read_memory_into(mem, ptr, &mut vec)?;
        Ok(vec)
    }

    pub fn set_u128(&mut self, mem: &wasmtime::Memory, ptr: usize, val: u128) -> Result<(), Trap> {
        self.write_into_memory(mem, ptr, &val.to_le_bytes())
    }

    pub fn termination_reason(&mut self) -> &mut TerminationReason {
        &mut self.caller.data_mut().termination_reason
    }
}

impl<'c, E: Ext> FunctionContext<E> for Context<'c, E> {
    type Error = Trap;
    type Memory = wasmtime::Memory;

    fn ext(&self) -> &E {
        &self.caller.data().ext
    }

    fn ext_mut(&mut self) -> &mut E {
        &mut self.caller.data_mut().ext
    }

    fn read_memory_into(
        &mut self,
        mem: &Self::Memory,
        offset: usize,
        buffer: &mut [u8],
    ) -> Result<(), Self::Error> {
        memory::read(&mut self.caller, mem, offset, buffer)
            .map_err(|e| Trap::new(FuncError::<E::Error>::from(e)))
    }

    fn write_into_memory(
        &mut self,
        mem: &Self::Memory,
        offset: usize,
        buffer: &[u8],
    ) -> Result<(), Self::Error> {
        memory::write(&mut self.caller, mem, offset, buffer)
            .map_err(|e| Trap::new(FuncError::<E::Error>::from(e)))
    }
}
