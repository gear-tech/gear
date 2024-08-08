//! Auxiliary implementation of the taskpool.

use std::{cell::RefCell, collections::btree_map::IntoIter};

use gear_core::ids::ProgramId;

use crate::{
    scheduler::{ScheduledTask, TaskPoolImpl},
    storage::{DoubleMapStorage, GetFirstPos, IteratorWrap, KeyIterableByKeyMap},
};

use super::DoubleBTreeMap;

pub type BlockNumber = u32;

pub type AuxiliaryTaskpool<TaskPoolCallbacks> = TaskPoolImpl<
    TaskPoolStorageWrap,
    ScheduledTask<ProgramId>,
    TaskPoolErrorImpl,
    TaskPoolErrorImpl,
    TaskPoolCallbacks,
>;

std::thread_local! {
    pub(crate) static TASKPOOL_STORAGE: RefCell<DoubleBTreeMap<BlockNumber, ScheduledTask<ProgramId>, ()>> = RefCell::new(DoubleBTreeMap::new());
}

pub struct TaskPoolStorageWrap;

impl DoubleMapStorage for TaskPoolStorageWrap {
    type Key1 = BlockNumber;
    type Key2 = ScheduledTask<ProgramId>;
    type Value = ();

    fn contains_keys(key1: &Self::Key1, key2: &Self::Key2) -> bool {
        TASKPOOL_STORAGE.with_borrow(|map| map.contains_keys(key1, key2))
    }

    fn get(key1: &Self::Key1, key2: &Self::Key2) -> Option<Self::Value> {
        TASKPOOL_STORAGE.with_borrow(|map| map.get(key1, key2).cloned())
    }

    fn insert(key1: Self::Key1, key2: Self::Key2, value: Self::Value) {
        TASKPOOL_STORAGE.with_borrow_mut(|map| map.insert(key1, key2, value));
    }

    fn mutate<R, F: FnOnce(&mut Option<Self::Value>) -> R>(
        _key1: Self::Key1,
        _key2: Self::Key2,
        _f: F,
    ) -> R {
        unimplemented!()
    }

    fn mutate_values<F: FnMut(Self::Value) -> Self::Value>(_f: F) {
        unimplemented!()
    }

    fn remove(key1: Self::Key1, key2: Self::Key2) {
        Self::take(key1, key2);
    }

    fn clear() {
        TASKPOOL_STORAGE.with_borrow_mut(|map| map.clear());
    }

    fn take(key1: Self::Key1, key2: Self::Key2) -> Option<Self::Value> {
        TASKPOOL_STORAGE.with_borrow_mut(|map| map.remove(key1, key2))
    }

    fn clear_prefix(_first_key: Self::Key1) {
        unimplemented!()
    }
}

impl KeyIterableByKeyMap for TaskPoolStorageWrap {
    type Key1 = BlockNumber;
    type Key2 = ScheduledTask<ProgramId>;

    type DrainIter =
        IteratorWrap<IntoIter<ScheduledTask<ProgramId>, ()>, ScheduledTask<ProgramId>, GetFirstPos>;

    type Iter =
        IteratorWrap<IntoIter<ScheduledTask<ProgramId>, ()>, ScheduledTask<ProgramId>, GetFirstPos>;

    fn drain_prefix_keys(key: Self::Key1) -> Self::DrainIter {
        TASKPOOL_STORAGE
            .with_borrow_mut(|map| map.drain_key(&key))
            .into()
    }

    fn iter_prefix_keys(key: Self::Key1) -> Self::Iter {
        TASKPOOL_STORAGE
            .with_borrow(|map| map.iter_key(&key))
            .into()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TaskPoolErrorImpl {
    /// Occurs when given task already exists in task pool.
    DuplicateTask,
    /// Occurs when task wasn't found in storage.
    TaskNotFound,
}

impl crate::scheduler::TaskPoolError for TaskPoolErrorImpl {
    fn duplicate_task() -> Self {
        Self::DuplicateTask
    }

    fn task_not_found() -> Self {
        Self::TaskNotFound
    }
}
