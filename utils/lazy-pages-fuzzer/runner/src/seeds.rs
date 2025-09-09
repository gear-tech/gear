// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use rand_chacha::{
    ChaCha20Rng,
    rand_core::{RngCore as _, SeedableRng},
};

use crate::{
    SEED_SIZE_IN_U32,
    utils::{cast_slice_mut, cast_vec},
};

pub fn generate_seed(timestamp: u64) -> Vec<u32> {
    // Expand the u64 timestamp to 32-byte seed (ChaCha20Rng expects 256-bit seed)
    let mut rng_seed = [0u8; 32];
    rng_seed[..8].copy_from_slice(&timestamp.to_le_bytes());

    let mut rng = ChaCha20Rng::from_seed(rng_seed);
    let mut buf = vec![0u32; SEED_SIZE_IN_U32];
    rng.fill_bytes(cast_slice_mut(&mut buf));
    buf
}

pub fn generate_instance_seed(timestamp: u64) -> [u8; 32] {
    // Expand the u64 timestamp to 32-byte seed (ChaCha20Rng expects 256-bit seed)
    let mut seed = [0u8; 32];
    seed[..8].copy_from_slice(&timestamp.to_le_bytes());

    // Generate a random instance seed
    let mut instance_seed = [0u8; 32];
    let mut rng = ChaCha20Rng::from_seed(seed);
    rng.fill_bytes(&mut instance_seed);
    instance_seed
}

pub fn derivate_seed(seed: &[u32], der: &[u8; 32]) -> Vec<u8> {
    // x ^= x << 13;
    // x ^= x >> 17;
    // x ^= x << 5;
    let mut new_seed = vec![0u32; seed.len()];
    for i in 0..seed.len() {
        new_seed[i] = seed[i].rotate_left(13)
            ^ u32::from_le_bytes([
                der[i % 32],
                der[(i + 1) % 32],
                der[(i + 2) % 32],
                der[(i + 3) % 32],
            ]);
        new_seed[i] ^= new_seed[i].rotate_right(17);
        new_seed[i] ^= new_seed[i].rotate_left(5);
    }

    cast_vec(new_seed)
}

#[cfg(test)]
mod tests {
    use crate::ts;

    use super::*;

    #[test]
    fn test_generate_seed() {
        let seed = generate_seed(ts());
        let seed2 = generate_seed(ts());
        assert_ne!(seed, seed2, "Generated seeds should be different");
    }

    #[test]
    fn test_generate_instance_seed() {
        let seed = generate_instance_seed(ts());
        let seed2 = generate_instance_seed(ts());
        assert_ne!(seed, seed2, "Generated seeds should be different");
    }

    #[test]
    fn test_derivate_seed() {
        let seed = generate_seed(ts());

        let instance_seed = generate_instance_seed(ts());
        let derived_seed = derivate_seed(&seed, &instance_seed);
        let instance_seed2 = generate_instance_seed(ts());
        let derived_seed2 = derivate_seed(&seed, &instance_seed2);
        assert_ne!(
            derived_seed, derived_seed2,
            "Derived seeds should be different"
        );
    }
}
