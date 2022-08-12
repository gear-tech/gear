// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

pub mod constants {
    use frame_support::{
        parameter_types,
        weights::{constants, RuntimeDbWeight},
    };

    parameter_types! {
        /// `ParityDB` can be enabled with a feature flag, but is still experimental. These weights
        /// are available for brave runtime engineers who may want to try this out as default.
        pub const ParityDbWeight: RuntimeDbWeight = RuntimeDbWeight {
            read: 8_000 * constants::WEIGHT_PER_NANOS,
            write: 50_000 * constants::WEIGHT_PER_NANOS,
        };
    }

    #[cfg(test)]
    mod test_db_weights {
        use super::constants::ParityDbWeight as W;
        use frame_support::weights::constants;

        /// Checks that all weights exist and have sane values.
        // NOTE: If this test fails but you are sure that the generated values are fine,
        // you can delete it.
        #[test]
        fn sane() {
            // At least 1 µs.
            assert!(
                W::get().reads(1) >= constants::WEIGHT_PER_MICROS,
                "Read weight should be at least 1 µs."
            );
            assert!(
                W::get().writes(1) >= constants::WEIGHT_PER_MICROS,
                "Write weight should be at least 1 µs."
            );
            // At most 1 ms.
            assert!(
                W::get().reads(1) <= constants::WEIGHT_PER_MILLIS,
                "Read weight should be at most 1 ms."
            );
            assert!(
                W::get().writes(1) <= constants::WEIGHT_PER_MILLIS,
                "Write weight should be at most 1 ms."
            );
        }
    }
}
