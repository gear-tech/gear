use std::{
    cell::{self, RefCell},
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering},
};

use sandbox_wasmer::{AsStoreMut, Store, StoreMut};

// Wrapper around `RefCell` which allows to manually return borrowed store.
// It ensures what's store can be only borrowed mutably once.
#[derive(Debug)]
pub struct StoreRefCell {
    inner: RefCell<Store>,
    is_store_returned: AtomicBool,
}

#[derive(Debug)]
pub struct GetStoreError;

impl StoreRefCell {
    // Create new store ref cell
    pub fn new(store: Store) -> Self {
        Self {
            inner: RefCell::new(store),
            is_store_returned: AtomicBool::new(false),
        }
    }

    pub fn replace(&self, store: Store) {
        self.inner.replace(store);
    }

    // Borrow store immutably, same semantics as `RefCell::borrow`
    #[track_caller]
    pub fn borrow(&self) -> Ref<'_> {
        if self.is_store_returned.load(Ordering::SeqCst) {
            // Safety: store was returned, so it's safe to borrow immutably
            let store = unsafe { &*self.inner.as_ptr() };

            Ref::Returned(store)
        } else {
            Ref::Normal(self.inner.borrow())
        }
    }

    // Borrow store mutably, same semantics as `RefCell::borrow_mut`
    #[track_caller]
    pub fn borrow_mut(&self) -> RefMut<'_> {
        if self.is_store_returned.load(Ordering::SeqCst) {
            // Safety: store was returned, so it's safe to borrow mutably
            let store = unsafe { &mut *self.inner.as_ptr() };

            RefMut::Returned(store)
        } else {
            RefMut::Normal(self.inner.borrow_mut())
        }
    }

    // Returns previously returned store
    #[track_caller]
    pub fn get_store(&self) -> Result<StoreMut<'_>, GetStoreError> {
        if self.is_store_returned.load(Ordering::SeqCst) {
            self.is_store_returned.store(false, Ordering::SeqCst);
            // Safety: store was returned, so it's safe to borrow mutably
            let store = unsafe { &mut *self.inner.as_ptr() };

            Ok(store.as_store_mut())
        } else {
            Err(GetStoreError)
        }
    }

    // Returns store
    #[track_caller]
    pub fn return_store(&self, _store: StoreMut) {
        self.is_store_returned.store(true, Ordering::SeqCst);
    }

    // Returns store ptr, same semantics as `RefCell::as_ptr`
    pub unsafe fn as_ptr(&self) -> *mut Store {
        self.inner.as_ptr()
    }
}

pub enum Ref<'a> {
    Returned(&'a Store),
    Normal(cell::Ref<'a, Store>),
}

impl<'a> Deref for Ref<'a> {
    type Target = Store;

    #[inline]
    fn deref(&self) -> &Self::Target {
        match self {
            Ref::Returned(store) => store,
            Ref::Normal(store) => store,
        }
    }
}

pub enum RefMut<'a> {
    Returned(&'a mut Store),
    Normal(cell::RefMut<'a, Store>),
}

impl<'a> Deref for RefMut<'a> {
    type Target = Store;

    #[inline]
    fn deref(&self) -> &Self::Target {
        match self {
            RefMut::Returned(store) => store,
            RefMut::Normal(store) => store,
        }
    }
}

impl DerefMut for RefMut<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            RefMut::Returned(store) => store,
            RefMut::Normal(store) => store,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;

    #[test]
    fn test_store_refcell_smoke() {
        let store = Store::default();
        let store_refcell = StoreRefCell::new(store);

        {
            let _ref1 = store_refcell.borrow();
            let _ref2 = store_refcell.borrow();
        }

        let _store_mut = store_refcell.borrow_mut();
    }

    #[test]
    fn test_store_refcell_return() {
        struct Env {
            store: Rc<StoreRefCell>,
        }

        let store = Store::default();
        let rc = Rc::new(StoreRefCell::new(store));
        let env = Env { store: rc.clone() };

        let callback = |env: Env, storemut: StoreMut| {
            // do something with `storemut`
            // ..
            env.store.return_store(storemut);

            // Syscall handler is called and it uses `rc`
            {
                let _borrow = rc.borrow_mut();
            }
            // Syscall returns

            let _storemut = env.store.get_store().unwrap();
            // do something with `storemut`
            // ..
        };

        let mut borrow = rc.borrow_mut();
        callback(env, borrow.as_store_mut())
    }
}
