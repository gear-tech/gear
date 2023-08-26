use crate::*;
use sp_runtime::traits::Get;

pub struct NominationPoolsMigrationV4OldPallet;
impl Get<Perbill> for NominationPoolsMigrationV4OldPallet {
    fn get() -> Perbill {
        Perbill::from_percent(10)
    }
}

/// All migrations that will run on the next runtime upgrade.
///
/// Should be cleared after every release.
pub type Migrations = (
    // unreleased
    pallet_nomination_pools::migration::v4::MigrateV3ToV5<
        Runtime,
        NominationPoolsMigrationV4OldPallet,
    >,
    pallet_offences::migration::v1::MigrateToV1<Runtime>,
);
