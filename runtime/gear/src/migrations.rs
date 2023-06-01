use crate::*;

use pallet_gear_gas::migrations::v1::MigrateToV1 as GasTreeMigration;
use runtime_common::migration::SessionValidatorSetMigration;

pub type Migrations = (
    SessionValidatorSetMigration<Runtime>,
    GasTreeMigration<Runtime>,
    pallet_gear_scheduler::migration::MigrateToV2<Runtime>,
    pallet_gear_program::migration::MigrateToV2<Runtime>,
);
