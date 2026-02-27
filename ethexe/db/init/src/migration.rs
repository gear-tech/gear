// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::InitConfig;
use anyhow::Result;
use ethexe_db::DatabaseRef;

pub trait Migration<'a> {
    fn migrate(
        &'a self,
        config: &'a InitConfig,
        db: &'a DatabaseRef<'a, 'a>,
    ) -> Box<dyn Future<Output = Result<()>> + 'a>;
}

impl<'a, Fut, F> Migration<'a> for F
where
    F: Fn(&'a InitConfig, &'a DatabaseRef<'a, 'a>) -> Fut,
    Fut: Future<Output = Result<()>> + 'a,
{
    fn migrate(
        &'a self,
        config: &'a InitConfig,
        db: &'a DatabaseRef<'a, 'a>,
    ) -> Box<dyn Future<Output = Result<()>> + 'a> {
        Box::new((self)(config, db))
    }
}

#[cfg(test)]
pub mod test {
    use indoc::formatdoc;
    use parity_scale_codec::Encode;
    use scale_info::{MetaType, PortableRegistry, Registry};
    use sha3::{Digest, Sha3_256};

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
