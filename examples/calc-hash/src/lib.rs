#![no_std]

extern crate alloc;

use codec::{Decode, Encode};
use sha2::Digest;

pub type PackageId = [u8; 32];

/// Calculation package.
#[derive(Clone, Debug, Encode, Decode)]
pub struct Package {
    /// Result of the calculation.
    pub result: [u8; 32],
    /// Current calculating times.
    pub counter: u128,
}

impl Package {
    /// New package
    pub fn new(src: [u8; 32]) -> Self {
        Package {
            result: src,
            counter: 0,
        }
    }

    /// Calculate the next path.
    pub fn calc(&mut self) {
        self.result = sha2_512_256(&self.result);
        self.counter += 1;
    }

    /// Check if the calculation is finished.
    pub fn finished(&self, expected: u128) -> bool {
        self.counter >= expected
    }
}

/// Do a sha2 256-bit hash and return result.
pub fn sha2_512_256(data: &[u8]) -> [u8; 32] {
    let mut output = [0u8; 32];
    output.copy_from_slice(sha2::Sha512_256::digest(data).as_slice());
    output
}

/// Path verification.
pub fn verify_result(mut src: [u8; 32], times: u128, result: [u8; 32]) -> bool {
    for _ in 0..times {
        src = sha2_512_256(&src);
    }

    src == result
}
