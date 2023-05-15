use crate::*;

use frame_support::traits::OnRuntimeUpgrade;
use pallet_gear_gas::migrations::v1::MigrateToV1 as GasTreeMigration;

pub struct SessionValidatorSetMigration;

impl OnRuntimeUpgrade for SessionValidatorSetMigration {
    fn on_runtime_upgrade() -> Weight {
        let current_validators = Session::validators();
        validator_set::Validators::<Runtime>::mutate(|v| *v = current_validators.clone());
        validator_set::ApprovedValidators::<Runtime>::mutate(|v| *v = current_validators);

        RuntimeBlockWeights::get().max_block
    }
}

pub type Migrations = (
    SessionValidatorSetMigration,
    GasTreeMigration<Runtime>,
    pallet_gear_scheduler::migration::MigrateToV2<Runtime>,
    pallet_gear_program::migration::MigrateToV2<Runtime>,
);
