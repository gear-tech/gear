//! Environment of the wasm execution
use crate::metadata::{result::Result, StoreData};
use wasmtime::{
    AsContext, AsContextMut, Caller, Extern, Func, Linker, Memory, MemoryType, Store, Trap,
};

/// Apply environment to wasm instance
pub fn apply(store: &mut Store<StoreData>, linker: &mut Linker<StoreData>) -> Result<()> {
    let memory = Memory::new(store.as_context_mut(), MemoryType::new(256, None))?;

    // Define memory
    linker.define("env", "memory", Extern::Memory(memory))?;

    // Define functions
    linker.define(
        "env",
        "alloc",
        Extern::Func(Func::wrap(
            store.as_context_mut(),
            move |mut caller: Caller<'_, StoreData>, pages: i32| {
                memory
                    .clone()
                    .grow(caller.as_context_mut(), pages as u64)
                    .map_err(|e| {
                        log::error!("{:?}", e);

                        Trap::i32_exit(1)
                    })
                    .map(|pages| pages as i32)
            },
        )),
    )?;

    linker.define(
        "env",
        "free",
        Extern::Func(Func::wrap(store.as_context_mut(), |_: i32| {})),
    )?;

    linker.define(
        "env",
        "gr_debug",
        Extern::Func(Func::wrap(
            store.as_context_mut(),
            move |caller: Caller<'_, StoreData>, ptr: i32, len: i32| {
                let (ptr, len) = (ptr as usize, len as usize);

                let mut msg = vec![0; len];
                memory
                    .clone()
                    .read(caller.as_context(), ptr, &mut msg)
                    .map_err(|e| {
                        log::error!("{:?}", e);
                        Trap::i32_exit(1)
                    })?;

                log::debug!("{:?}", String::from_utf8_lossy(&msg));
                Ok(())
            },
        )),
    )?;

    linker.define(
        "env",
        "gr_size",
        Extern::Func(Func::wrap(
            store.as_context_mut(),
            |caller: Caller<'_, StoreData>| caller.data().msg.len() as i32,
        )),
    )?;

    linker.define(
        "env",
        "gr_read",
        Extern::Func(Func::wrap(
            store.as_context_mut(),
            move |mut caller: Caller<'_, StoreData>, ptr: i32, len: i32, dest: i32| {
                let (ptr, len, dest) = (ptr as usize, len as usize, dest as usize);

                let mut msg = vec![0; len];
                msg.copy_from_slice(&caller.data().msg[ptr..(ptr + len)]);

                memory
                    .clone()
                    .write(caller.as_context_mut(), dest, &msg)
                    .map_err(|e| {
                        log::error!("{:?}", e);

                        Trap::i32_exit(1)
                    })?;

                Ok(())
            },
        )),
    )?;

    Ok(())
}
