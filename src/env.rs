//! Wasmtime environment for running a module
use std::cell::{RefCell, RefMut};
use std::rc::Rc;

use codec::{Decode, Encode};

use crate::memory::PageNumber;
use crate::message::OutgoingMessage;
use crate::program::ProgramId;
use ::anyhow::{self, anyhow};
use wasmtime::{unix::StoreExt, Engine, Extern, Func, Instance, Memory, Module};

fn handle_sigsegv<E: Ext + 'static>(
    ext: &LaterExt<E>,
    mut touched: RefMut<Vec<(PageNumber, PageAction, *const u8)>>,
    base: *mut u8,
    signum: libc::c_int,
    siginfo: *const libc::siginfo_t,
) -> bool {
    // SIGSEGV on Linux, SIGBUS on Mac
    if libc::SIGSEGV == signum || libc::SIGBUS == signum {
        let si_addr: *mut libc::c_void = unsafe { (*siginfo).si_addr() };

        // Any signal from within module's memory we handle ourselves
        let length = 65536;
        let page = ((si_addr as usize) - (base as usize)) / length;

        // Set the base address of the page that the program is trying to access
        let page_base = base.wrapping_add(page * length);

        let access = ext.with(|ext: &mut E| ext.memory_access((page as u32).into()));
        if let Some(last) = touched.last_mut() {
            if last.2 == (si_addr as *const u8).wrapping_sub(base as usize)
                && last.1 == PageAction::Read
                && access == PageAction::Write
            {
                *last = (
                    last.0,
                    PageAction::Write,
                    (si_addr as *const u8).wrapping_sub(base as usize),
                );
                // Remove protections so the execution may resume
                unsafe {
                    libc::mprotect(
                        page_base as *mut libc::c_void,
                        length,
                        libc::PROT_READ | libc::PROT_WRITE,
                    );
                }
                log::debug!("MEMORY: ACCESS PAGE {} WRITE", page);

                true
            } else {
                // Set READ prrotection
                unsafe {
                    libc::mprotect(page_base as *mut libc::c_void, length, libc::PROT_READ);
                }
                touched.push((
                    (page as u32).into(),
                    PageAction::Read,
                    (si_addr as *const u8).wrapping_sub(base as usize),
                ));

                true
            }
        } else if access != PageAction::None {
            // Set READ prrotection
            unsafe {
                libc::mprotect(page_base as *mut libc::c_void, length, libc::PROT_READ);
            }
            touched.push((
                (page as u32).into(),
                PageAction::Read,
                (si_addr as *const u8).wrapping_sub(base as usize),
            ));

            true
        } else {
            touched.push((
                (page as u32).into(),
                PageAction::None,
                (si_addr as *const u8).wrapping_sub(base as usize),
            ));

            false
        }
    } else {
        // Otherwise, we forward to wasmtime's signal handler.
        false
    }
}

#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, Copy)]
pub enum PageAction {
    Read,
    Write,
    None,
}

pub trait Ext {
    fn alloc(&mut self, pages: PageNumber) -> Result<PageNumber, &'static str>;
    fn send(&mut self, msg: OutgoingMessage) -> Result<(), &'static str>;
    fn source(&mut self) -> Option<ProgramId>;
    fn free(&mut self, ptr: PageNumber) -> Result<(), &'static str>;
    fn debug(&mut self, data: &str) -> Result<(), &'static str>;
    fn set_mem(&mut self, ptr: usize, val: &[u8]) -> Result<(), &'static str>;
    fn get_mem(&mut self, ptr: usize, buffer: &mut [u8]) -> Result<(), &'static str>;
    fn msg(&mut self) -> &[u8];
    fn memory_access(&self, page: PageNumber) -> PageAction;
    fn memory_lock(&self);
    fn memory_unlock(&self);
}

struct LaterExt<E: Ext> {
    inner: Rc<RefCell<Option<E>>>,
}

impl<E: Ext> Clone for LaterExt<E> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<E: Ext> LaterExt<E> {
    fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(None)),
        }
    }

    fn set(&mut self, e: E) {
        *self.inner.borrow_mut() = Some(e)
    }

    fn with<R>(&self, f: impl FnOnce(&mut E) -> R) -> R {
        let mut brw = self.inner.borrow_mut();
        let mut ext = brw
            .take()
            .expect("with should be called only when inner is set");
        let res = f(&mut ext);

        *brw = Some(ext);

        res
    }

    fn unset(&mut self) -> E {
        self.inner
            .borrow_mut()
            .take()
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
                    _ => {
                        return Ok(0u32);
                    }
                };

                log::debug!("ALLOC: {} pages at {}", pages, ptr);

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
                    if let Err(_) = ext.with(|ext: &mut E| {
                        let data = {
                            let mut data = vec![0; message_len];
                            ext.get_mem(message_ptr, data.as_mut_slice())
                                .map_err(|_e| log::error!("Read memory err: {}", _e))
                                .ok();
                            data
                        };
                        ext.send(OutgoingMessage::new(
                            ProgramId(program_id as _),
                            data.into(),
                        ))
                    }) {
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
                    log::debug!("FREE ERROR: {:?}", e);
                } else {
                    log::debug!("FREE: {}", page);
                }
                Ok(())
            })
        };

        let size = {
            let ext = ext.clone();
            Func::wrap(&store, move || {
                ext.with(|ext: &mut E| ext.msg().len() as isize as i32)
            })
        };

        let read = {
            let ext = ext.clone();
            Func::wrap(&store, move |at: i32, len: i32, dest: i32| {
                let at = at as u32 as usize;
                let len = len as u32 as usize;
                let dest = dest as u32 as usize;
                ext.with(|ext: &mut E| {
                    let msg = ext.msg().to_vec();
                    ext.set_mem(dest, &msg[at..at + len])
                        .map_err(|_e| log::error!("Write memory err: {}", _e))
                        .ok();
                });
                Ok(())
            })
        };

        let debug = {
            let ext = ext.clone();
            Func::wrap(&store, move |str_ptr: i32, str_len: i32| {
                let str_ptr = str_ptr as u32 as usize;
                let str_len = str_len as u32 as usize;
                ext.with(|ext: &mut E| {
                    let debug_str = unsafe {
                        let mut debug_str = vec![0; str_len];
                        ext.get_mem(str_ptr, debug_str.as_mut_slice())
                            .map_err(|_e| log::error!("Read memory err: {}", _e))
                            .ok();
                        String::from_utf8_unchecked(debug_str)
                    };
                    log::debug!("DEBUG: {}", debug_str);
                });

                Ok(())
            })
        };

        let source = {
            let ext = ext.clone();
            Func::wrap(&store, move || {
                Ok(ext
                    .with(|ext: &mut E| ext.source())
                    .map(|v| v.0)
                    .unwrap_or_default())
            })
        };

        Self {
            store,
            ext,
            alloc,
            send,
            free,
            size,
            read,
            debug,
            source,
        }
    }

    fn run_inner(
        &mut self,
        module: Module,
        static_area: Vec<u8>,
        memory: Memory,
        func: impl FnOnce(Instance) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let mut imports = module
            .imports()
            .map(|import| {
                if import.module() != "env" {
                    Err(anyhow!("Non-env imports are not supported"))
                } else {
                    Ok((import.name(), Option::<Extern>::None))
                }
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        for (ref import_name, ref mut ext) in imports.iter_mut() {
            *ext = if import_name == &Some("send") {
                Some(self.send.clone().into())
            } else if import_name == &Some("source") {
                Some(self.source.clone().into())
            } else if import_name == &Some("alloc") {
                Some(self.alloc.clone().into())
            } else if import_name == &Some("free") {
                Some(self.free.clone().into())
            } else if import_name == &Some("size") {
                Some(self.size.clone().into())
            } else if import_name == &Some("read") {
                Some(self.read.clone().into())
            } else if import_name == &Some("debug") {
                Some(self.debug.clone().into())
            } else if import_name == &Some("memory") {
                Some(memory.clone().into())
            } else {
                continue;
            };
        }

        let externs = imports
            .into_iter()
            .map(|(_, host_function)| host_function.ok_or_else(|| anyhow!("Missing import")))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let instance = Instance::new(&self.store, &module, &externs)?;

        unsafe {
            memory.data_unchecked_mut()[0..static_area.len()].copy_from_slice(&static_area[..])
        };

        func(instance)
    }

    pub fn setup_and_run(
        &mut self,
        ext: E,
        module: Module,
        static_area: Vec<u8>,
        memory: Memory,
        func: impl FnOnce(Instance) -> anyhow::Result<()>,
    ) -> (anyhow::Result<()>, E, Vec<(PageNumber, PageAction)>) {
        // Lock memory
        ext.memory_lock();

        self.ext.set(ext);
        let touched: Rc<RefCell<Vec<(PageNumber, PageAction, *const u8)>>> =
            Rc::new(RefCell::new(Vec::new()));
        let touched_clone = touched.clone();

        let ext_clone = self.ext.clone();
        let base = memory.data_ptr();

        unsafe {
            self.store.set_signal_handler(move |signum, siginfo, _| {
                handle_sigsegv(
                    &ext_clone,
                    touched_clone.borrow_mut(),
                    base,
                    signum,
                    siginfo,
                )
            });
        }

        let result = self.run_inner(module, static_area, memory, func);

        let ext = self.ext.unset();

        // Unlock memory
        ext.memory_unlock();
        let touched = touched.take().iter().map(|(a, b, _)| (*a, *b)).collect();

        (result, ext, touched)
    }

    pub fn engine(&self) -> &Engine {
        self.store.engine()
    }

    pub fn create_memory(&self, total_pages: u32) -> Memory {
        Memory::new(
            &self.store,
            wasmtime::MemoryType::new(wasmtime::Limits::at_least(total_pages)),
        )
    }
}
