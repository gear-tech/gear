//! Wasmtime environment for running a module
use std::rc::Rc;
use std::cell::RefCell;

use crate::memory::PageNumber;
use crate::message::OutgoingMessage;
use crate::program::ProgramId;
use wasmtime::{Func, Module, Instance, Memory, Extern, Engine};
use ::anyhow::{anyhow, self};

pub trait Ext {
    fn alloc(&mut self, pages: PageNumber) -> Result<PageNumber, &'static str>;
    fn send(&mut self, msg: OutgoingMessage) -> Result<(), &'static str>;
    fn source(&mut self) -> Option<ProgramId>;
    fn free(&mut self, ptr: PageNumber) -> Result<(), &'static str>;
    fn debug(&mut self, data: &str) -> Result<(), &'static str>;
    fn set_mem(&mut self, ptr: usize, val: &[u8]);
    fn get_mem(&mut self, ptr: usize, len: usize) -> &[u8];
    fn msg(&mut self) -> &[u8];
}

struct LaterExt<E: Ext> {
    inner: Rc<RefCell<Option<E>>>,
}

impl<E: Ext> Clone for LaterExt<E> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl<E: Ext> LaterExt<E> {
    fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(None))
        }
    }

    fn set(&mut self, e: E) {
        *self.inner.borrow_mut() = Some(e)
    }

    fn with<R>(&self, f: impl FnOnce(&mut E) -> R) -> R {
        let mut brw = self.inner.borrow_mut();
        let mut ext = brw.take().expect("with should be called only when inner is set");
        let res = f(&mut ext);

        *brw = Some(ext);

        res
    }

    fn unset(&mut self) -> E {
        self.inner.borrow_mut().take()
            .expect("Unset should be paired with set and called after")
    }
}

/// Environment to run one module at a time providing Ext.
pub struct Environment<E: Ext + 'static> {
    store: wasmtime::Store,
    ext: LaterExt<E>,
    send: Func,
    source: Func,
    alloc: Func,
    free: Func,
    size: Func,
    read: Func,
    debug: Func,
}

impl<E: Ext + 'static> Environment<E> {
    pub fn new() -> Self {
        let store = wasmtime::Store::default();

        let ext = LaterExt::new();

        let alloc = {
            let ext = ext.clone();
            Func::wrap(&store, move |pages: i32| {
                let pages = pages as u32;

                let ptr = match ext.with(|ext: &mut E| ext.alloc(pages.into())) {
                    Ok(ptr) => ptr.raw(),
                    _ => { return Ok(0u32); }
                };

                println!("ALLOC: {} pages at {}", pages, ptr);

                Ok(ptr)
            })
        };

        let send = {
            let ext = ext.clone();
            Func::wrap(
                &store,
                move |program_id: i64, message_ptr: i32, message_len: i32| {
                    let message_ptr = message_ptr as u32 as usize;
                    let message_len = message_len as u32 as usize;
                    if let Err(_) = ext.with(
                        |ext: &mut E| {
                            let data = ext.get_mem(message_ptr, message_len).to_vec();
                            ext.send(OutgoingMessage::new(ProgramId(program_id as _), data.into()))
                        }
                    ) {
                        return Err(wasmtime::Trap::new("Trapping: unable to send message"));
                    }

                    Ok(())
                },
            )
        };

        let free = {
            let ext = ext.clone();
            Func::wrap(&store, move |page: i32| {
                let page = page as u32;
                if let Err(e) = ext.with(|ext: &mut E| ext.free(page.into())) {
                    println!("FREE ERROR: {:?}", e);
                } else {
                    println!("FREE: {}", page);
                }
                Ok(())
            })
        };

        let size = {
            let ext = ext.clone();
            Func::wrap(&store, move || ext.with(|ext: &mut E| ext.msg().len() as isize as i32))
        };

        let read = {
            let ext = ext.clone();
            Func::wrap(&store, move |at: i32, len: i32, dest: i32| {
                let at = at as u32 as usize;
                let len = len as u32 as usize;
                let dest = dest as u32 as usize;
                ext.with(|ext: &mut E| {
                    let msg = ext.msg().to_vec();
                    ext.set_mem(dest, &msg[at..at+len]);
                });
                Ok(())
            })
        };

        let debug = {
            let ext = ext.clone();
            Func::wrap(
                &store,
                move |str_ptr: i32, str_len: i32| {
                    let str_ptr = str_ptr as u32 as usize;
                    let str_len = str_len as u32 as usize;
                    ext.with(|ext: &mut E| {
                        let debug_str = unsafe { String::from_utf8_unchecked(ext.get_mem(str_ptr, str_len).to_vec()) };
                        println!("DEBUG: {}", debug_str);
                    });

                    Ok(())
                },
            )
        };

        let source = {
            let ext = ext.clone();
            Func::wrap(&store, move || {
                Ok(ext.with(|ext: &mut E| ext.source()).map(|v| v.0).unwrap_or_default())
            })
        };

        Self { store, ext, alloc, send, free, size, read, debug, source }
    }

    fn run_inner(
        &mut self,
        module: Module,
        memory: Memory,
        func: impl FnOnce(Instance) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let mut imports = module
            .imports()
            .map(
                |import| if import.module() != "env" {
                    return Err(anyhow!("Non-env imports are not supported"))
                } else {
                    Ok((import.name(), Option::<Extern>::None))
                }
            )
            .collect::<anyhow::Result<Vec<_>>>()?;

        for (ref import_name, ref mut ext) in imports.iter_mut() {
            *ext = if import_name == &"send" {
                Some(self.send.clone().into())
            } else if import_name == &"source" {
                Some(self.source.clone().into())
            } else if import_name == &"alloc" {
                Some(self.alloc.clone().into())
            } else if import_name == &"free" {
                Some(self.free.clone().into())
            } else if import_name == &"size" {
                Some(self.size.clone().into())
            } else if import_name == &"read" {
                Some(self.read.clone().into())
            } else if import_name == &"debug" {
                Some(self.debug.clone().into())
            } else if import_name == &"memory" {
                Some(memory.clone().into())
            } else {
                continue;
            };
        }

        let externs = imports
            .into_iter()
            .map(|(_, host_function)| host_function.ok_or(anyhow!("Missing import")))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let instance = Instance::new(
            &self.store,
            &module,
            &externs,
        )?;

        func(instance)
    }

    pub fn setup_and_run(
        &mut self,
        ext: E,
        module: Module,
        memory: Memory,
        func: impl FnOnce(Instance) -> anyhow::Result<()>,
    ) -> (anyhow::Result<()>, E) {
        self.ext.set(ext);

        let result = self.run_inner(module, memory, func);

        let ext = self.ext.unset();

        (result, ext)
    }

    pub fn engine(&self) -> &Engine {
        self.store.engine()
    }

    pub fn create_memory(&self, total_pages: u32) -> Memory {
        Memory::new(
            &self.store,
            wasmtime::MemoryType::new(
                wasmtime::Limits::at_least(total_pages)
            ),
        )
    }
}
