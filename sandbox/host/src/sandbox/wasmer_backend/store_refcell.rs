// This file is part of Gear.

// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! # Description
//!
//! Custom implementation of `RefCell` for the `sandbox_wasmer::Store` type,
//! enabling safe repeated mutable borrowing of `StoreRefCell` higher up the call stack
//! when the mutable borrow of `StoreRefCell` still exists.
//!
//! Example illustrating functionality in terms of `RefCell` from the standard library:
//!
//! At first we borrow store mutably:
//!
//! ```ignore
//!    let refcell = RefCell::new(Store::default());
//!    let mut_borrow = refcell.borrow_mut();
//!
//!    func(&refcell, &mut mut_borrow);
//! ```
//!
//! Now we need to borrow store mutably again inside `func`,
//! but we can't do it because `mut_borrow` still exists.
//!  
//! ```ignore
//!    fn func(ref_cell: &RefCell<Store>, mut_borrow: &mut Store) {
//!        ref_cell.borrow_mut(); // This will panic
//!   }
//! ```
//!  
//! With `StoreRefCell` we can do it safely:
//!
//! ```ignore
//!    fn func(store_refcell: &StoreRefCell, mut_borrow: &mut Store) {
//!        store_refcell.borrow_scope(mut_borrow, || {
//!            // Now we can borrow store again
//!            let second_mut_borrow = store_refcell.borrow_mut();
//!        });
//!   }
//! ```
//!  
//! # Why is this necessary? Can't we do without repeated mutable borrowing?
//!
//! The issue arises because when handling syscalls within an instance of a program running in Wasmer,
//! a runtime interface call occurs, leading to a situation where we have two nested runtime interface calls.
//! The first call `sandbox::invoke` initiates the program execution, the second occurs during the syscall processing.
//!
//! Thus, the call stack at the highest point looks like this:
//!
//! ```text
//!   -----------------------------------
//!   | Memory::write                   | Write sandbox memory (Borrows Store mutably)
//!   ---------native boundary-----------
//!   | sandbox::memory_set             | Runtime on behalf of processing syscall make a call to runtime interface
//!   -----------------------------------
//!   | runtime executes syscall        |
//!   --------runtime boundary-----------
//!   | syscall_callback                | Wasmer calls syscall callback from inside his VM
//!   -----------------------------------
//!   | Wasmer's Func::call             | Sandbox starts to executes program function (Borrows Store mutably)
//!   -------native boundary----------- |
//!   | sandbox::invoke                 | Runtime interface call
//!   -----------------------------------
//! ```
//!
//! As we can see, the `sandbox::invoke` function borrows the store mutably,
//! and then the `sandbox::memory_set` runtime interface call borrows the store mutably again.
//!
//! Therefore, since it is not possible to pass a reference to Store through nested runtime interface call
//! or cancel previous mutable borrow, it is necessary to use `StoreRefCell` for safe repeated mutable borrowing of `Store`.
//!

use std::{
    cell::{Cell, UnsafeCell},
    num::NonZeroUsize,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use defer::defer;
use sandbox_wasmer::{AsStoreMut, AsStoreRef, Store, StoreRef};

#[derive(Debug, Clone, Copy)]
enum BorrowState {
    Shared(NonZeroUsize),
    Mutable,
    NonShared,
}

/// Custom implementation of `RefCell` which allows to safely borrow store
/// mutably/immutably second time inside the scope.
#[derive(Debug)]
pub struct StoreRefCell {
    store: UnsafeCell<Store>,
    state: Cell<BorrowState>,
}

#[derive(Debug)]
pub struct BorrowScopeError;

impl StoreRefCell {
    /// Create new `StoreRefCell` with provided `Store`
    pub fn new(store: Store) -> Self {
        Self {
            store: UnsafeCell::new(store),
            state: Cell::new(BorrowState::NonShared),
        }
    }

    /// Borrow store immutably, same semantics as `RefCell::borrow`
    #[track_caller]
    pub fn borrow(&self) -> Ref<'_> {
        match self.state.get() {
            BorrowState::Shared(n) => {
                self.state.set(BorrowState::Shared(
                    NonZeroUsize::new(n.get() + 1).expect("non zero"),
                ));
            }
            BorrowState::NonShared => {
                self.state
                    .set(BorrowState::Shared(NonZeroUsize::new(1).expect("non zero")));
            }
            BorrowState::Mutable => {
                panic!("store already borrowed mutably");
            }
        }

        Ref {
            store: NonNull::new(self.store.get()).expect("non null"),
            state: &self.state,
        }
    }

    /// Borrow store mutably, same semantics as `RefCell::borrow_mut`
    #[track_caller]
    pub fn borrow_mut(&self) -> RefMut<'_> {
        match self.state.get() {
            BorrowState::NonShared => {
                self.state.set(BorrowState::Mutable);
            }
            BorrowState::Shared(_) | BorrowState::Mutable => {
                panic!("store already borrowed");
            }
        }

        RefMut {
            store: NonNull::new(self.store.get()).expect("non null"),
            state: &self.state,
        }
    }

    /// Provide borrow scope where store can be borrowed mutably second time safely (or borrowed immutably multiple times).
    pub fn borrow_scope<R, F: FnOnce() -> R>(
        &self,
        store: impl AsStoreMut,
        f: F,
    ) -> Result<R, BorrowScopeError> {
        // We expect  the same store
        debug_assert!(
            self.compare_stores(store.as_store_ref()),
            "stores are different"
        );

        // Caller just returned borrowed mutably reference to the store, now we can safely borrow it mutably again
        let _store = store;

        // We received a mutable borrow, so other states shouldn't be possible
        if let BorrowState::Shared(_) | BorrowState::NonShared = self.state.get() {
            return Err(BorrowScopeError);
        }

        self.state.set(BorrowState::NonShared);

        let result = f();

        // We expect that after scope ends, store won't be borrowed
        debug_assert!(matches!(self.state.get(), BorrowState::NonShared));

        // Restore previous state after scope ends
        defer!(self.state.set(BorrowState::Mutable));

        Ok(result)
    }

    #[allow(unused)]
    fn compare_stores(&self, returned_store: StoreRef) -> bool {
        // SAFETY:
        // Verified with Miri, it seems safe.
        // Carefully compare the stores while don't using/holding mutable references to them in the same time.
        let orig_store_ref: StoreRef = unsafe { &*self.store.get() }.as_store_ref();

        StoreRef::same(&orig_store_ref, &returned_store)
    }

    /// Returns store ptr, same semantics as `RefCell::as_ptr`
    pub unsafe fn as_ptr(&self) -> *mut Store {
        self.store.get()
    }
}

pub struct Ref<'b> {
    store: NonNull<Store>,
    state: &'b Cell<BorrowState>,
}

impl Deref for Ref<'_> {
    type Target = Store;

    #[inline]
    fn deref(&self) -> &Self::Target {
        // SAFETY: we ensure that store isn't borrowed mutably before
        unsafe { self.store.as_ref() }
    }
}

impl Drop for Ref<'_> {
    fn drop(&mut self) {
        match self.state.get() {
            BorrowState::Shared(n) if n.get() == 1 => {
                self.state.set(BorrowState::NonShared);
            }
            BorrowState::Shared(n) => {
                self.state.set(BorrowState::Shared(
                    NonZeroUsize::new(n.get() - 1).expect("non zero"),
                ));
            }
            _ => unreachable!(),
        }
    }
}

pub struct RefMut<'b> {
    store: NonNull<Store>,
    state: &'b Cell<BorrowState>,
}

impl<'a> Deref for RefMut<'a> {
    type Target = Store;

    #[inline]
    fn deref(&self) -> &Self::Target {
        // SAFETY: we ensure that store isn't borrowed before
        unsafe { self.store.as_ref() }
    }
}

impl DerefMut for RefMut<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: we ensure that store isn't borrowed before
        unsafe { self.store.as_mut() }
    }
}

impl Drop for RefMut<'_> {
    fn drop(&mut self) {
        match self.state.get() {
            BorrowState::Mutable => {
                self.state.set(BorrowState::NonShared);
            }
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use sandbox_wasmer::StoreMut;

    use super::*;

    #[test]
    fn test_store_refcell_borrow() {
        let store = Store::default();
        let store_refcell = StoreRefCell::new(store);

        {
            let _borrow = store_refcell.borrow();
            let _borrow = store_refcell.borrow();
        }
        {
            let _borrow = store_refcell.borrow_mut();
        }
        {
            let _borrow = store_refcell.borrow();
            let _borrow = store_refcell.borrow();
        }
    }

    #[test]
    fn test_store_refcell_borrow_scope() {
        struct Env {
            store: Rc<StoreRefCell>,
        }

        let store = Store::default();
        let rc = Rc::new(StoreRefCell::new(store));
        let env = Env { store: rc.clone() };

        let callback = |env: Env, mut storemut: StoreMut| {
            // do something with `storemut`
            // ..

            let rc = rc.clone();
            let _ = env.store.borrow_scope(&mut storemut, move || {
                // Callback is called and it allowed to borrow store mutably/immutably
                {
                    let _borrow = rc.borrow_mut();
                }
                {
                    let _borrow = rc.borrow();
                    let _borrow = rc.borrow();
                }
                {
                    let _borrow = rc.borrow_mut();
                }
            });

            // do something with `storemut`
            // ..
            let _ = storemut;
        };

        let mut borrow = rc.borrow_mut();
        callback(env, borrow.as_store_mut())
    }
}
