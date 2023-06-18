// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

/// Currency related constants
pub mod currency {
    use runtime_primitives::Balance;

    /// The existential deposit.
    pub const EXISTENTIAL_DEPOSIT: Balance = 10_000_000_000_000; // 10 Vara

    /// The program rent cost per block.
    pub const RENT_COST_PER_BLOCK: Balance = 125_000_000;

    // TODO: review quantities based on economic model (issue #1277)
    pub const UNITS: Balance = 1_000_000_000_000; // 10^(-12) precision
    pub const DOLLARS: Balance = UNITS * 20; // 1 token is worth ~$0.05
    pub const CENTS: Balance = DOLLARS / 100; // 200_000_000_000
    pub const MILLICENTS: Balance = CENTS / 1_000; // 200_000_000

    /// Helper function to calculate various deposits for using pallets' storage
    pub const fn deposit(items: u32, bytes: u32) -> Balance {
        // TODO: review numbers (#2650)
        items as Balance * 15 * CENTS + (bytes as Balance) * 6 * CENTS
    }
}

/// Time and block constants
pub mod time {
    use runtime_primitives::{BlockNumber, Moment};

    /// Since BABE is probabilistic this is the average expected block time that
    /// we are targetting. Blocks will be produced at a minimum duration defined
    /// by `SLOT_DURATION`, but some slots will not be allocated to any
    /// authority and hence no block will be produced. We expect to have this
    /// block time on average following the defined slot duration and the value
    /// of `c` configured for BABE (where `1 - c` represents the probability of
    /// a slot being empty).
    /// This value is only used indirectly to define the unit constants below
    /// that are expressed in blocks. The rest of the code should use
    /// `SLOT_DURATION` instead (like the Timestamp pallet for calculating the
    /// minimum period).
    ///
    /// If using BABE with secondary slots (default) then all of the slots will
    /// always be assigned, in which case `MILLISECS_PER_BLOCK` and
    /// `SLOT_DURATION` should have the same value.
    ///
    /// <https://research.web3.foundation/en/latest/polkadot/BABE/Babe/#6-practical-results>

    pub const MILLISECS_PER_BLOCK: Moment = 3000;

    // Milliseconds per year for the Julian year (365.25 days).
    pub const MILLISECONDS_PER_YEAR: u64 = 1000 * 3600 * 24 * 36525 / 100;

    // NOTE: Currently it is not possible to change the slot duration after the chain has started.
    //       Attempting to do so will brick block production.
    pub const SLOT_DURATION: Moment = MILLISECS_PER_BLOCK;

    // Time is measured by number of blocks.
    pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
    pub const HOURS: BlockNumber = MINUTES * 60;
    pub const DAYS: BlockNumber = HOURS * 24;
    pub const WEEKS: BlockNumber = DAYS * 7;

    // NOTE: Currently it is not possible to change the epoch duration after the chain has started.
    //       Attempting to do so will brick block production.
    pub const EPOCH_DURATION_IN_BLOCKS: BlockNumber = 2 * HOURS;
    pub const EPOCH_DURATION_IN_SLOTS: u64 = {
        const SLOT_FILL_RATE: f64 = MILLISECS_PER_BLOCK as f64 / SLOT_DURATION as f64;

        (EPOCH_DURATION_IN_BLOCKS as f64 * SLOT_FILL_RATE) as u64
    };

    // 1 in 4 blocks (on average, not counting collisions) will be primary BABE blocks.
    pub const PRIMARY_PROBABILITY: (u64, u64) = (1, 4);

    // The free of charge period of rent.
    pub const RENT_FREE_PERIOD: BlockNumber = pallet_gear_program::migration::FREE_PERIOD;
}
