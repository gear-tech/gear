//! Host functions
use crate::metadata::{result::Result, StoreData};
use wasmtime::{
    AsContext, AsContextMut, Caller, Extern, Func, Linker, Memory, MemoryType, Store, Trap,
};

pub fn alloc(ctx: impl AsContextMut<Data = StoreData>, memory: Memory) -> Extern {
    Extern::Func(Func::wrap(
        ctx,
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
    ))
}

pub fn free(ctx: impl AsContextMut<Data = StoreData>) -> Extern {
    Extern::Func(Func::wrap(ctx, |_: i32| {}))
}

pub fn gr_debug(ctx: impl AsContextMut<Data = StoreData>, memory: Memory) -> Extern {
    Extern::Func(Func::wrap(
        ctx,
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
    ))
}

pub fn gr_read(ctx: impl AsContextMut<Data = StoreData>, memory: Memory) -> Extern {
    Extern::Func(Func::wrap(
        ctx,
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
    ))
}

/// # NOTE
///
/// Just for the compatible with the program metadata
pub fn gr_reply(ctx: impl AsContextMut<Data = StoreData>, _memory: Memory) -> Extern {
    Extern::Func(Func::wrap(
        ctx,
        move |mut _caller: Caller<'_, StoreData>, _ptr: i32, _len: i32, _val: i32, _msg: i32| 0,
    ))
}

/// # NOTE
///
/// Just for the compatible with the program metadata
pub fn gr_error(ctx: impl AsContextMut<Data = StoreData>, _memory: Memory) -> Extern {
    Extern::Func(Func::wrap(
        ctx,
        move |mut _caller: Caller<'_, StoreData>, _ptr: i32| Ok(()),
    ))
}

pub fn gr_size(ctx: impl AsContextMut<Data = StoreData>) -> Extern {
    Extern::Func(Func::wrap(ctx, |caller: Caller<'_, StoreData>| {
        caller.data().msg.len() as i32
    }))
}
