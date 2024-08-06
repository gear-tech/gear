use gear_common::{
    auxiliary::task_pool::{
        AuxiliaryTaskpool, BlockNumber, TaskPoolErrorImpl, TaskPoolStorageWrap,
    },
    scheduler::{ScheduledTask, TaskPool, TaskPoolCallbacks},
    storage::KeyIterableByKeyMap,
    ProgramId,
};

#[derive(Debug, Default)]
pub(crate) struct TaskPoolManager;

impl TaskPoolManager {
    pub(crate) fn add(
        &self,
        block_number: BlockNumber,
        task: ScheduledTask<ProgramId>,
    ) -> Result<(), TaskPoolErrorImpl> {
        <AuxiliaryTaskpool<TaskPoolCallbacksImpl> as TaskPool>::add(block_number, task)
    }

    pub(crate) fn clear(&self) {
        <AuxiliaryTaskpool<TaskPoolCallbacksImpl> as TaskPool>::clear();
    }

    pub(crate) fn contains(
        &self,
        block_number: &BlockNumber,
        task: &ScheduledTask<ProgramId>,
    ) -> bool {
        <AuxiliaryTaskpool<TaskPoolCallbacksImpl> as TaskPool>::contains(block_number, task)
    }

    pub(crate) fn delete(
        &self,
        block_number: BlockNumber,
        task: ScheduledTask<ProgramId>,
    ) -> Result<(), TaskPoolErrorImpl> {
        <AuxiliaryTaskpool<TaskPoolCallbacksImpl> as TaskPool>::delete(block_number, task)
    }

    pub(crate) fn drain(
        &self,
        block_number: BlockNumber,
    ) -> <TaskPoolStorageWrap as KeyIterableByKeyMap>::DrainIter {
        AuxiliaryTaskpool::<TaskPoolCallbacksImpl>::drain_prefix_keys(block_number)
    }
}

pub(crate) struct TaskPoolCallbacksImpl;

impl TaskPoolCallbacks for TaskPoolCallbacksImpl {
    type OnAdd = ();
    type OnDelete = ();
}
