#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, Encode};
use sha2::Digest;

pub type PackageId = [u8; 32];

/// Calculation package.
#[derive(Clone, Debug, Encode, Decode)]
pub struct Package {
    /// Paths of the calculation.
    pub paths: Vec<[u8; 32]>,
    /// Expected result.
    pub expected: [u8; 32],
}

/// Package with id specified.
#[derive(Clone, Debug, Encode, Decode)]
pub struct PackageWithId {
    /// The Id of calculation pacakge.
    pub id: PackageId,
    /// Calculation package.
    pub package: Package,
}

impl Package {
    /// Path verification.
    pub fn verify(path: &[[u8; 32]]) -> bool {
        let len = path.len();
        for (i, p) in path.iter().enumerate() {
            let next = i + 1;
            if next == len {
                return true;
            }

            if sha2_256(p) != path[next] {
                return false;
            }
        }

        false
    }

    /// Calculate the next path.
    pub fn calc(mut self) -> Self {
        self.paths.push(sha2_256(&self.ptr()));
        self
    }

    /// Check if the calculation is finished.
    pub fn finished(&self) -> bool {
        self.ptr() == self.expected
    }

    fn ptr(&self) -> [u8; 32] {
        *self.paths.last().expect("Invalid route.")
    }
}

/// Do a sha2 256-bit hash and return result.
pub fn sha2_256(data: &[u8]) -> [u8; 32] {
    let mut output = [0u8; 32];
    output.copy_from_slice(sha2::Sha256::digest(data).as_slice());
    output
}
