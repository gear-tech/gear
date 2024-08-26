use gear_common::{auxiliary::BlockNumber, scheduler::StorageType, LockId, MessageId};

use crate::RESERVE_FOR;

use super::ExtManager;

/// Hold bound, specifying cost of storage, expected block number for task to
/// create on it, deadlines and durations of holding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HoldBound {
    cost: u64,
    expected: BlockNumber,
    lock_id: Option<LockId>,
}

impl HoldBound {
    pub fn cost(&self) -> u64 {
        self.cost
    }

    pub fn expected(&self) -> BlockNumber {
        self.expected
    }

    pub fn lock_id(&self) -> Option<LockId> {
        self.lock_id
    }

    pub fn expected_duration(&self, manager: &ExtManager) -> BlockNumber {
        self.expected.saturating_sub(manager.block_height())
    }

    pub fn deadline(&self) -> BlockNumber {
        self.expected.saturating_add(RESERVE_FOR)
    }

    pub fn deadline_duration(&self, manager: &ExtManager) -> BlockNumber {
        self.deadline().saturating_sub(manager.block_height())
    }

    pub fn lock_amount(&self, manager: &ExtManager) -> u64 {
        let duration: u64 = self.deadline_duration(manager).into();
        duration.saturating_mul(self.cost())
    }
}

pub struct HoldBoundBuilder {
    storage_type: StorageType,
    cost: u64,
}

impl HoldBoundBuilder {
    pub fn new(storage_type: StorageType) -> Self {
        Self {
            storage_type,
            cost: ExtManager::cost_by_storage_type(storage_type),
        }
    }

    pub fn at(self, expected: BlockNumber) -> HoldBound {
        HoldBound {
            cost: self.cost,
            expected,
            lock_id: self.storage_type.try_into().ok(),
        }
    }

    pub fn deadline(self, deadline: BlockNumber) -> HoldBound {
        let expected = deadline.saturating_sub(RESERVE_FOR);

        self.at(expected)
    }

    pub fn duration(self, manager: &ExtManager, duration: BlockNumber) -> HoldBound {
        let expected = manager.block_height().saturating_add(duration);

        self.at(expected)
    }

    pub fn maximum_for(self, manager: &ExtManager, gas: u64) -> HoldBound {
        let deadline_duration = gas
            .saturating_div(self.cost.max(1))
            .try_into()
            .expect("not sane deadline");
        let deadline = manager
            .blocks_manager
            .get()
            .height
            .saturating_add(deadline_duration);

        self.deadline(deadline)
    }

    pub fn maximum_for_message(self, manager: &ExtManager, message_id: MessageId) -> HoldBound {
        let gas_limit = manager.gas_tree.get_limit(message_id).unwrap_or_else(|e| {
            let err_msg = format!(
                "HoldBoundBuilder::maximum_for_message: failed getting message gas limit. \
                Message id - {message_id}. Got error - {e:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}");
        });

        self.maximum_for(manager, gas_limit)
    }
}
