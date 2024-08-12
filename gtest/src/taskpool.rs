use gear_common::{
    auxiliary::{
        taskpool::{AuxiliaryTaskpool, TaskPoolErrorImpl, TaskPoolStorageWrap},
        BlockNumber,
    },
    scheduler::{ScheduledTask, TaskPool, TaskPoolCallbacks},
    storage::KeyIterableByKeyMap,
    ProgramId,
};

/// Task pool manager for gtest environment.
///
/// TODO(ap): wait for #4119 and work on integrating this into it, until then
/// allow(dead_code).
#[derive(Debug, Default)]
#[allow(dead_code)]
pub(crate) struct TaskPoolManager;

#[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use gear_common::{scheduler::ScheduledTask, ProgramId};

    use super::TaskPoolManager;

    #[test]
    fn test_taskpool() {
        let manager = TaskPoolManager;

        let block_1_tasks = [
            ScheduledTask::<ProgramId>::SendDispatch(42.into()),
            ScheduledTask::<ProgramId>::SendUserMessage {
                message_id: 422.into(),
                to_mailbox: true,
            },
        ];
        let block_2_tasks = [
            ScheduledTask::<ProgramId>::RemoveGasReservation(922.into(), 1.into()),
            ScheduledTask::<ProgramId>::RemoveFromWaitlist(42.into(), 44.into()),
        ];

        block_1_tasks
            .iter()
            .for_each(|task| manager.add(1, task.clone()).unwrap());

        block_2_tasks
            .iter()
            .for_each(|task| manager.add(2, task.clone()).unwrap());

        for task in block_1_tasks.iter() {
            assert!(manager.contains(&1, task));
        }

        for task in block_2_tasks.iter() {
            assert!(manager.contains(&2, task));
        }

        for task in manager.drain(1) {
            assert!(
                block_1_tasks.contains(&task),
                "task not found in block 1 tasks"
            );
        }

        for task in manager.drain(2) {
            assert!(
                block_2_tasks.contains(&task),
                "task not found in block 2 tasks"
            );
        }

        for task in block_1_tasks.iter() {
            assert!(!manager.contains(&1, task));
        }

        for task in block_2_tasks.iter() {
            assert!(!manager.contains(&2, task));
        }

        let task = ScheduledTask::<ProgramId>::RemoveFromMailbox(422.into(), 16.into());
        manager.add(3, task.clone()).unwrap();
        manager.add(4, task.clone()).unwrap();
        manager.clear();
        manager.delete(4, task.clone()).unwrap();
        assert!(!manager.contains(&3, &task));
        assert!(!manager.contains(&4, &task));
    }
}
