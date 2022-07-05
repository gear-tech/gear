#![no_std]

extern crate alloc;

use codec::{Decode, Encode};
use sha2::Digest;

pub type PackageId = [u8; 32];

/// Calculation package.
#[derive(Clone, Debug, Encode, Decode)]
pub struct Package {
    /// Id of the package.
    pub id: PackageId,
    /// Result of the calculation.
    pub result: [u8; 32],
    /// Current calculating times.
    pub counter: u128,
    /// Expected calcuating times.
    pub expected: u128,
}

impl Package {
    /// New package
    pub fn new(src: [u8; 32], id: &[u8], expected: u128) -> Self {
        Package {
            id: sha2_256(&id),
            result: src,
            counter: 0,
            expected,
        }
    }

    /// Path verification.
    pub fn verify(mut src: [u8; 32], times: u128, result: [u8; 32]) -> bool {
        for _ in 0..times {
            src = sha2_256(&src);
        }

        src == result
    }

    /// Calculate the next path.
    pub fn calc(&mut self) {
        self.result = sha2_256(&self.result);
        self.counter += 1;
    }

    /// Check if the calculation is finished.
    pub fn finished(&self) -> bool {
        self.counter == self.expected
    }
}

/// Do a sha2 256-bit hash and return result.
pub fn sha2_256(data: &[u8]) -> [u8; 32] {
    let mut output = [0u8; 32];
    output.copy_from_slice(sha2::Sha256::digest(data).as_slice());
    output
}
