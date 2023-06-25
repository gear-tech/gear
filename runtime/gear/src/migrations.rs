use crate::*;

pub type Migrations = (
    pallet_gear_gas::migrations::v2::MigrateToV2<Runtime>,
    pallet_gear_messenger::migrations::MigrateToV2<Runtime>,
);
