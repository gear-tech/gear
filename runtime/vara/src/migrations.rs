use crate::*;

use frame_support::traits::OnRuntimeUpgrade;

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
    pallet_gear_program::migration::MigrateV1ToV2<Runtime>,
);
