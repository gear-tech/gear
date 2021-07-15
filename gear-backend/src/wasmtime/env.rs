//! Wasmtime environment for running a module.

use wasmtime::{Engine, Extern, Func, Instance, Module, Store};

use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;

use ::anyhow::{self, anyhow};

use super::memory::MemoryWrap;

use gear_core::env::{Ext, LaterExt, PageAction, PageInfo};
use gear_core::memory::{Memory, PageNumber};
use gear_core::message::{OutgoingPacket, ReplyPacket};
use gear_core::program::ProgramId;
/// Environment to run one module at a time providing Ext.
pub struct Environment<E: Ext + 'static> {
    store: wasmtime::Store,
    ext: LaterExt<E>,
    funcs: BTreeMap<&'static str, Func>,
}

impl<E: Ext + 'static> Environment<E> {
    /// New environment.
    ///
    /// To run actual function with provided external environment, `setup_and_run` should be used.
    pub fn new() -> Self {
        let store = Store::default();
        let ext = LaterExt::new();

        let mut funcs = BTreeMap::new();
        funcs.insert("alloc", Func::wrap(&store, Self::func_alloc(ext.clone())));
        funcs.insert("free", Func::wrap(&store, Self::func_free(ext.clone())));
        funcs.insert("gas", Func::wrap(&store, Self::func_gas(ext.clone())));
        funcs.insert(
            "gr_charge",
            Func::wrap(&store, Self::func_charge(ext.clone())),
        );
        funcs.insert(
            "gr_debug",
            Func::wrap(&store, Self::func_debug(ext.clone())),
        );
        funcs.insert(
            "gr_msg_id",
            Func::wrap(&store, Self::func_msg_id(ext.clone())),
        );
        funcs.insert("gr_read", Func::wrap(&store, Self::func_read(ext.clone())));
        funcs.insert(
            "gr_reply",
            Func::wrap(&store, Self::func_reply(ext.clone())),
        );
        funcs.insert("gr_send", Func::wrap(&store, Self::func_send(ext.clone())));
        funcs.insert("gr_size", Func::wrap(&store, Self::func_size(ext.clone())));
        funcs.insert(
            "gr_source",
            Func::wrap(&store, Self::func_source(ext.clone())),
        );
        funcs.insert(
            "gr_value",
            Func::wrap(&store, Self::func_value(ext.clone())),
        );

        Self { store, ext, funcs }
    }

    /// Setup external environment and run closure.
    ///
    /// Setup external environment by providing `ext`, run nenwly initialized instance created from
    /// provided `module`, do anything inside a `func` delegate.
    ///
    /// This will also set the beginning of the memory region to the `static_area` content _after_
    /// creatig instance.
    pub fn setup_and_run(
        &mut self,
        ext: E,
        binary: &[u8],
        static_area: Vec<u8>,
        memory: &dyn Memory,
        entry_point: &str,
    ) -> (anyhow::Result<()>, E, Vec<(PageNumber, PageAction)>) {
        let module = Module::new(self.store.engine(), binary).expect("Error creating module");
        let touched: Rc<RefCell<Vec<PageInfo>>> = Rc::new(RefCell::new(Vec::new()));

        cfg_if::cfg_if! {
            if #[cfg(target_os = "linux")] {
                use wasmtime::unix::StoreExt;

                // Lock memory
                ext.memory_lock();

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
            }
        }

        self.ext.set(ext);

        let result = self.run_inner(module, static_area, memory, move |instance| {
            instance
                .get_func(entry_point)
                .ok_or_else(|| {
                    anyhow::format_err!("failed to find `{}` function export", entry_point)
                })
                .and_then(|entry_func| entry_func.call(&[]))
                .map(|_| ())
        });

        let ext = self.ext.unset();
        cfg_if::cfg_if! {
            if #[cfg(target_os = "linux")] {

                // Unlock memory
                ext.memory_unlock();
            }
        }

        let touched = touched.take().iter().map(|(a, b, _)| (*a, *b)).collect();

        (result, ext, touched)
    }

    /// Return engine used by this environment.
    pub fn engine(&self) -> &Engine {
        self.store.engine()
    }

    /// Create memory inside this environment.
    pub fn create_memory(&self, total_pages: u32) -> MemoryWrap {
        MemoryWrap::new(
            wasmtime::Memory::new(
                &self.store,
                wasmtime::MemoryType::new(wasmtime::Limits::at_least(total_pages)),
            )
            .expect("Create env memory fail"),
        )
    }

    fn run_inner(
        &mut self,
        module: Module,
        static_area: Vec<u8>,
        memory: &dyn Memory,
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
            if let Some(name) = import_name {
                *ext = match *name {
                    "memory" => {
                        let mem: &wasmtime::Memory =
                            match memory.as_any().downcast_ref::<wasmtime::Memory>() {
                                Some(mem) => mem,
                                None => panic!("Memory is not wasmtime::Memory"),
                            };
                        Some(wasmtime::Extern::Memory(Clone::clone(mem)))
                    }
                    key if self.funcs.contains_key(key) => Some(self.funcs[key].clone().into()),
                    _ => continue,
                }
            }
        }

        let externs = imports
            .into_iter()
            .map(|(_, host_function)| host_function.ok_or_else(|| anyhow!("Missing import")))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let instance = Instance::new(&self.store, &module, &externs)?;

        memory.write(0, &static_area).expect("Err write mem");

        func(instance)
    }

    fn func_alloc(ext: LaterExt<E>) -> impl Fn(i32) -> Result<u32, wasmtime::Trap> {
        move |pages: i32| {
            let pages = pages as u32;

            let ptr = match ext.with(|ext: &mut E| ext.alloc(pages.into())) {
                Ok(ptr) => ptr.raw(),
                _ => return Ok(0u32),
            };

            log::debug!("ALLOC: {} pages at {}", pages, ptr);

            Ok(ptr)
        }
    }

    fn func_free(ext: LaterExt<E>) -> impl Fn(i32) -> Result<(), wasmtime::Trap> {
        move |page: i32| {
            let page = page as u32;
            if let Err(e) = ext.with(|ext: &mut E| ext.free(page.into())) {
                log::debug!("FREE ERROR: {:?}", e);
            } else {
                log::debug!("FREE: {}", page);
            }
            Ok(())
        }
    }

    fn func_charge(ext: LaterExt<E>) -> impl Fn(i64) -> Result<(), wasmtime::Trap> {
        move |gas: i64| {
            if ext.with(|ext: &mut E| ext.charge(gas as u64)).is_err() {
                Err(wasmtime::Trap::new(
                    "Trapping: unable to charge gas for reserve",
                ))
            } else {
                Ok(())
            }
        }
    }

    fn func_gas(ext: LaterExt<E>) -> impl Fn(i32) -> Result<(), wasmtime::Trap> {
        move |val: i32| {
            if ext.with(|ext: &mut E| ext.gas(val as _)).is_err() {
                Err(wasmtime::Trap::new(
                    "Trapping: unable to report about gas used",
                ))
            } else {
                Ok(())
            }
        }
    }

    fn func_debug(ext: LaterExt<E>) -> impl Fn(i32, i32) -> Result<(), wasmtime::Trap> {
        move |str_ptr: i32, str_len: i32| {
            let str_ptr = str_ptr as u32 as usize;
            let str_len = str_len as u32 as usize;
            ext.with(|ext: &mut E| {
                let mut data = vec![0u8; str_len];
                ext.get_mem(str_ptr, &mut data);
                let debug_str = unsafe { String::from_utf8_unchecked(data) };
                log::debug!("DEBUG: {}", debug_str);
            });
            Ok(())
        }
    }

    fn func_msg_id(ext: LaterExt<E>) -> impl Fn(i32) -> Result<(), wasmtime::Trap> {
        move |msg_id_ptr: i32| {
            ext.with(|ext: &mut E| {
                let message_id = ext.message_id();
                ext.set_mem(msg_id_ptr as isize as _, message_id.as_slice());
            });
            Ok(())
        }
    }

    fn func_read(ext: LaterExt<E>) -> impl Fn(i32, i32, i32) -> Result<(), wasmtime::Trap> {
        move |at: i32, len: i32, dest: i32| {
            let at = at as u32 as usize;
            let len = len as u32 as usize;
            let dest = dest as u32 as usize;
            ext.with(|ext: &mut E| {
                let msg = ext.msg().to_vec();
                ext.set_mem(dest, &msg[at..at + len]);
            });
            Ok(())
        }
    }

    fn func_reply(ext: LaterExt<E>) -> impl Fn(i32, i32, i64, i32) -> Result<(), wasmtime::Trap> {
        move |message_ptr: i32, message_len: i32, gas_limit: i64, value_ptr: i32| {
            let message_ptr = message_ptr as u32 as usize;
            let message_len = message_len as u32 as usize;
            let result = ext.with(|ext: &mut E| {
                let mut data = vec![0u8; message_len];
                ext.get_mem(message_ptr, &mut data);

                let mut value_le = [0u8; 16];
                ext.get_mem(value_ptr as isize as _, &mut value_le);

                ext.reply(ReplyPacket::new(
                    data.into(),
                    gas_limit as _,
                    u128::from_le_bytes(value_le),
                ))
            });

            if result.is_err() {
                return Err(wasmtime::Trap::new("Trapping: unable to send message"));
            }

            Ok(())
        }
    }

    fn func_send(
        ext: LaterExt<E>,
    ) -> impl Fn(i32, i32, i32, i64, i32) -> Result<(), wasmtime::Trap> {
        move |program_id_ptr: i32,
              message_ptr: i32,
              message_len: i32,
              gas_limit: i64,
              value_ptr: i32| {
            let message_ptr = message_ptr as u32 as usize;
            let message_len = message_len as u32 as usize;
            let result = ext.with(|ext: &mut E| {
                let mut data = vec![0u8; message_len];
                ext.get_mem(message_ptr, &mut data);
                let mut program_id = [0u8; 32];
                ext.get_mem(program_id_ptr as isize as _, &mut program_id);
                let program_id = ProgramId::from_slice(&program_id);

                let mut value_le = [0u8; 16];
                ext.get_mem(value_ptr as isize as _, &mut value_le);

                ext.send(OutgoingPacket::new(
                    program_id,
                    data.into(),
                    gas_limit as _,
                    u128::from_le_bytes(value_le),
                ))
            });

            if result.is_err() {
                return Err(wasmtime::Trap::new("Trapping: unable to send message"));
            }

            Ok(())
        }
    }

    fn func_size(ext: LaterExt<E>) -> impl Fn() -> i32 {
        move || ext.with(|ext: &mut E| ext.msg().len() as isize as i32)
    }

    fn func_source(ext: LaterExt<E>) -> impl Fn(i32) -> Result<(), wasmtime::Trap> {
        move |source_ptr: i32| {
            ext.with(|ext: &mut E| {
                let source = ext.source();
                ext.set_mem(source_ptr as isize as _, source.as_slice());
            });
            Ok(())
        }
    }

    fn func_value(ext: LaterExt<E>) -> impl Fn(i32) -> Result<(), wasmtime::Trap> {
        move |value_ptr: i32| {
            ext.with(|ext: &mut E| {
                let source = ext.value();
                ext.set_mem(value_ptr as isize as _, &source.to_le_bytes()[..]);
            });
            Ok(())
        }
    }
}

impl<E: Ext + 'static> Default for Environment<E> {
    /// Creates a default environment.
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "linux")]
fn handle_sigsegv<E: Ext + 'static>(
    ext: &LaterExt<E>,
    mut touched: core::cell::RefMut<Vec<PageInfo>>,
    base: *mut u8,
    signum: libc::c_int,
    siginfo: *const libc::siginfo_t,
) -> bool {
    // SIGSEGV on Linux
    if libc::SIGSEGV == signum {
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
