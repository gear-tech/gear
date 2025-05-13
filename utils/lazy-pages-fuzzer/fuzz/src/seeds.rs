use rand_chacha::{
    rand_core::{RngCore as _, SeedableRng},
    ChaCha20Rng,
};

pub fn generate_seed(timestamp: u64) -> Vec<u8> {
    // Expand the u64 timestamp to 32-byte seed (ChaCha20Rng expects 256-bit seed)
    let mut seed = [0u8; 32];
    seed[..8].copy_from_slice(&timestamp.to_le_bytes());

    let mut rng = ChaCha20Rng::from_seed(seed);
    let mut buf = vec![0u8; 35_000];
    rng.fill_bytes(&mut buf);
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

pub fn derivate_seed(seed: &[u8], der: &[u8; 32]) -> Vec<u8> {
    // x ^= x << 13;
    // x ^= x >> 17;
    // x ^= x << 5;
    let mut new_seed = vec![0u8; seed.len()];
    for i in 0..seed.len() {
        new_seed[i] = seed[i].rotate_left(13) ^ der[i % 32];
        new_seed[i] ^= new_seed[i].rotate_right(17);
        new_seed[i] ^= new_seed[i].rotate_left(5);
    }

    new_seed
}
