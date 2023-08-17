use crate::*;

pub type Migrations = (
    pallet_gear_gas::migrations::v1::MigrateToV1<Runtime>,
    pallet_gear_scheduler::migration::MigrateToV2<Runtime>,
    pallet_gear_gas::migrations::v2::MigrateToV2<Runtime>,
    pallet_gear_messenger::migrations::MigrateToV2<Runtime>,
    runtime_common::migrations::MigrateToGearBank<Runtime, GasConverter>,
);
