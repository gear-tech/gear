//! Wasmtime environment for running a module.

use wasmtime::{Engine, Extern, Func, Instance, Module, Store, Trap};

use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

use ::anyhow::{self, anyhow};

use super::memory::MemoryWrap;

use gear_core::env::{Ext, LaterExt, PageAction, PageInfo};
use gear_core::memory::{Memory, PageNumber};

use crate::funcs;
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
        let mut result = Self {
            store: Store::default(),
            ext: LaterExt::new(),
            funcs: BTreeMap::new(),
        };

        result.add_func_i32_to_u32("alloc", funcs::alloc);
        result.add_func_i32("free", funcs::free);
        result.add_func_i32("gas", funcs::gas);
        result.add_func_i32("gr_commit", funcs::commit);
        result.add_func_i64("gr_charge", funcs::charge);
        result.add_func_i32_i32("gr_debug", funcs::debug);
        result.add_func_i32_i32_i32_i64_i32_to_i32("gr_init", funcs::init);
        result.add_func_i32("gr_msg_id", funcs::msg_id);
        result.add_func_i32_i32_i32("gr_push", funcs::push);
        result.add_func_i32_i32_i32("gr_read", funcs::read);
        result.add_func_i32_i32_i64_i32("gr_reply", funcs::reply);
        result.add_func_i32_i32_i32_i64_i32("gr_send", funcs::send);
        result.add_func_to_i32("gr_size", funcs::size);
        result.add_func_i32("gr_source", funcs::source);
        result.add_func_i32("gr_value", funcs::value);

        result
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
        let touched: Rc<RefCell<Vec<PageInfo>>> = Default::default();

        cfg_if::cfg_if! {
            if #[cfg(target_os = "linux")] {
                use wasmtime::unix::StoreExt;

                // Lock memory
                ext.memory_lock();

                let touched_clone = Rc::clone(&touched);
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

    fn add_func_i32<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn(i32) -> Result<(), &'static str>,
    {
        self.funcs.insert(
            key,
            Func::wrap(&self.store, Self::wrap1(func(self.ext.clone()))),
        );
    }

    fn add_func_i32_i32<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn(i32, i32) -> Result<(), &'static str>,
    {
        self.funcs.insert(
            key,
            Func::wrap(&self.store, Self::wrap2(func(self.ext.clone()))),
        );
    }

    fn add_func_i32_i32_i32<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn(i32, i32, i32) -> Result<(), &'static str>,
    {
        self.funcs.insert(
            key,
            Func::wrap(&self.store, Self::wrap3(func(self.ext.clone()))),
        );
    }

    fn add_func_i32_i32_i32_i64_i32_to_i32<F>(
        &mut self,
        key: &'static str,
        func: fn(LaterExt<E>) -> F,
    ) where
        F: 'static + Fn(i32, i32, i32, i64, i32) -> Result<i32, &'static str>,
    {
        self.funcs.insert(
            key,
            Func::wrap(&self.store, Self::wrap5(func(self.ext.clone()))),
        );
    }

    fn add_func_i32_i32_i32_i64_i32<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn(i32, i32, i32, i64, i32) -> Result<(), &'static str>,
    {
        self.funcs.insert(
            key,
            Func::wrap(&self.store, Self::wrap5(func(self.ext.clone()))),
        );
    }

    fn add_func_i32_i32_i64_i32<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn(i32, i32, i64, i32) -> Result<(), &'static str>,
    {
        self.funcs.insert(
            key,
            Func::wrap(&self.store, Self::wrap4(func(self.ext.clone()))),
        );
    }

    fn add_func_i32_to_u32<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn(i32) -> Result<u32, &'static str>,
    {
        self.funcs.insert(
            key,
            Func::wrap(&self.store, Self::wrap1(func(self.ext.clone()))),
        );
    }

    fn add_func_i64<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn(i64) -> Result<(), &'static str>,
    {
        self.funcs.insert(
            key,
            Func::wrap(&self.store, Self::wrap1(func(self.ext.clone()))),
        );
    }

    fn add_func_to_i32<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn() -> i32,
    {
        self.funcs
            .insert(key, Func::wrap(&self.store, func(self.ext.clone())));
    }

    fn wrap1<T, R>(func: impl Fn(T) -> Result<R, &'static str>) -> impl Fn(T) -> Result<R, Trap> {
        move |a| func(a).map_err(Trap::new)
    }

    fn wrap2<T0, T1, R>(
        func: impl Fn(T0, T1) -> Result<R, &'static str>,
    ) -> impl Fn(T0, T1) -> Result<R, Trap> {
        move |a, b| func(a, b).map_err(Trap::new)
    }

    fn wrap3<T0, T1, T2, R>(
        func: impl Fn(T0, T1, T2) -> Result<R, &'static str>,
    ) -> impl Fn(T0, T1, T2) -> Result<R, Trap> {
        move |a, b, c| func(a, b, c).map_err(Trap::new)
    }

    fn wrap4<T0, T1, T2, T3, R>(
        func: impl Fn(T0, T1, T2, T3) -> Result<R, &'static str>,
    ) -> impl Fn(T0, T1, T2, T3) -> Result<R, Trap> {
        move |a, b, c, d| func(a, b, c, d).map_err(Trap::new)
    }

    fn wrap5<T0, T1, T2, T3, T4, R>(
        func: impl Fn(T0, T1, T2, T3, T4) -> Result<R, &'static str>,
    ) -> impl Fn(T0, T1, T2, T3, T4) -> Result<R, Trap> {
        move |a, b, c, d, e| func(a, b, c, d, e).map_err(Trap::new)
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
