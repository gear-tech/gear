use crate::storage::primitives::ValueStorage;
use core::marker::PhantomData;

pub trait Toggler {
    fn allow();

    fn allowed() -> bool;

    fn deny();

    fn denied() -> bool {
        !Self::allowed()
    }
}

pub struct TogglerImpl<VS: ValueStorage>(PhantomData<VS>);

impl<VS: ValueStorage<Value = bool>> Toggler for TogglerImpl<VS> {
    fn allow() {
        VS::put(true);
    }

    fn allowed() -> bool {
        VS::get() == Some(true)
    }

    fn deny() {
        VS::put(false);
    }
}
