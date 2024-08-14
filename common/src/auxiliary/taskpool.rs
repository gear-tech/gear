//! Auxiliary implementation of the taskpool.

use super::{AuxiliaryDoubleStorageWrap, BlockNumber, DoubleBTreeMap};
use crate::scheduler::{ScheduledTask, TaskPoolImpl};
use gear_core::ids::ProgramId;
use std::cell::RefCell;

pub type AuxiliaryTaskpool<TaskPoolCallbacks> = TaskPoolImpl<
    TaskPoolStorageWrap,
    ScheduledTask<ProgramId>,
    TaskPoolErrorImpl,
    TaskPoolErrorImpl,
    TaskPoolCallbacks,
>;

std::thread_local! {
    pub(crate) static TASKPOOL_STORAGE: RefCell<DoubleBTreeMap<BlockNumber, ScheduledTask<ProgramId>, ()>> = const { RefCell::new(DoubleBTreeMap::new()) };
}

pub struct TaskPoolStorageWrap;

impl AuxiliaryDoubleStorageWrap for TaskPoolStorageWrap {
    type Key1 = BlockNumber;
    type Key2 = ScheduledTask<ProgramId>;
    type Value = ();

    fn with_storage<F, R>(f: F) -> R
    where
        F: FnOnce(&DoubleBTreeMap<Self::Key1, Self::Key2, Self::Value>) -> R,
    {
        TASKPOOL_STORAGE.with_borrow(f)
    }

    fn with_storage_mut<F, R>(f: F) -> R
    where
        F: FnOnce(&mut DoubleBTreeMap<Self::Key1, Self::Key2, Self::Value>) -> R,
    {
        TASKPOOL_STORAGE.with_borrow_mut(f)
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
