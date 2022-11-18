//! Environment of the wasm execution
use crate::metadata::{funcs, result::Result, StoreData};
use wasmtime::{
    AsContext, AsContextMut, Caller, Extern, Func, Linker, Memory, MemoryType, Store, Trap,
};

/// Apply environment to wasm instance
pub fn apply(store: &mut Store<StoreData>, linker: &mut Linker<StoreData>) -> Result<()> {
    let memory = Memory::new(store.as_context_mut(), MemoryType::new(256, None))?;

    // Define memory
    linker.define("env", "memory", Extern::Memory(memory))?;

    // Define functions
    linker.define("env", "alloc", funcs::alloc(store.as_context_mut(), memory))?;
    linker.define("env", "free", funcs::free(store.as_context_mut()))?;

    linker.define(
        "env",
        "gr_debug",
        funcs::gr_debug(store.as_context_mut(), memory),
    )?;

    linker.define(
        "env",
        "gr_read",
        funcs::gr_read(store.as_context_mut(), memory),
    )?;

    linker.define(
        "env",
        "gr_reply",
        funcs::gr_reply(store.as_context_mut(), memory),
    )?;

    linker.define(
        "env",
        "gr_error",
        funcs::gr_error(store.as_context_mut(), memory),
    )?;

    linker.define(
        "env",
        "gr_size",
        funcs::gr_size(store.as_context_mut(), memory),
    )?;

    Ok(())
}
