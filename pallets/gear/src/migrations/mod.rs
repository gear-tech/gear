// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

#[cfg(test)]
#[cfg(feature = "try-runtime")]
mod tests {
    use crate::mock::*;

    use frame_support::traits::{OnRuntimeUpgrade, StorageVersion};

    #[test]
    fn test_context_store_migration_works() {
        new_test_ext().execute_with(|| {
            StorageVersion::new(3).put::<GearMessenger>();
            let state = pallet_gear_messenger::migrations::context_store::RemoveCommitStorage::<Test>::pre_upgrade().unwrap();
            let _w = pallet_gear_messenger::migrations::context_store::RemoveCommitStorage::<Test>::on_runtime_upgrade();
            pallet_gear_messenger::migrations::context_store::RemoveCommitStorage::<Test>::post_upgrade(state).unwrap();

        });
    }
}
