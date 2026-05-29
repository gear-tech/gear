// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Per-step database migration trait.
//!
//! Implementations live next to the version they upgrade *from* — e.g.
//! `v1::migration_from_v0` produces a v1 database from a v0 one. The
//! driver in [`super::migrate`] walks [`super::MIGRATIONS`] in order,
//! applying each step whose `source_version` matches the on-disk one.

use super::InitConfig;
use crate::RawDatabase;
use anyhow::Result;
use std::pin::Pin;

/// A single schema upgrade step. Implementations must be idempotent on
/// the migration's target version: running the same migration twice
/// must not corrupt a database that's already at
/// `source_version + 1`.
pub trait Migration: Sync {
    /// Schema version this migration upgrades from. Successful
    /// application leaves the database at `source_version() + 1`.
    fn source_version(&self) -> u32;

    /// Apply the migration in-place.
    fn migrate<'a>(
        &'a self,
        config: &'a InitConfig,
        db: &'a RawDatabase,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>>;
}

#[cfg(test)]
pub(super) mod test {
    use indoc::formatdoc;
    use parity_scale_codec::Encode;
    use scale_info::{MetaType, PortableRegistry, Registry};
    use sha3::{Digest, Sha3_256};

    #[allow(unused)]
    #[track_caller]
    pub fn assert_migration_types_hash(migration: &str, types: Vec<MetaType>, expected_hash: &str) {
        let mut registry = Registry::new();
        registry.register_types(types);

        let portable_registry = PortableRegistry::from(registry);
        let encoded_registry = portable_registry.encode();
        let type_info_hash = hex::encode(Sha3_256::digest(encoded_registry));

        if type_info_hash != expected_hash {
            panic!(
                "{}",
                formatdoc!(
                    "
                    Some of database types used in {migration} migration has been changed.

                    It can break the very migration process between database version.

                    It's generally OK to change these types as long as you
                    sure that it won't break the database itself, but must be
                    done carefully. If you know what exactly has been changed
                    and sure about it, please do the following steps:

                    - Check whether anything has been really changed.

                      This test can have false positives, e.g. when
                      some documentation has been changed, or changes
                      doesn't affect type encoding.

                      If nothing has been really changed and you're
                      totally sure about it, update the expected hash
                      in the text and skip the next step.

                    - If something has been really changed, you must
                      prevent the migration from using changed types,
                      as it can break the migration. Migrations update
                      the database between (possibly old) versions, so
                      types they use must be the same as on these
                      database versions.

                      So you have to save the old definitions for the migration.

                      Put copies of the previous type definitions you've
                      changed into `ethexe/db/init/src/v{{VERSION}}.rs`,
                      depending on the database version that introduces
                      the type. Change the migration code to ensure that
                      it uses that old versions instead of changed ones.
                      Then run the test again and update the expected hash
                      in the test.

                    Expected hash: {expected_hash}
                    Found hash:    {type_info_hash}
                    "
                )
            )
        }
    }
}
