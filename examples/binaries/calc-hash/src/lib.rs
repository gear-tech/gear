#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, Encode};
use sha2::Digest;

pub type PackageId = [u8; 32];

/// Methods of the aggregator of calc hash
#[derive(Debug, Encode, Decode)]
pub enum Method<ProgramId> {
    Start(Package),
    Refuel([u8; 32]),
    ForceInOneBlock(Package),
    SetCalculators(Calculators<ProgramId>),
}

/// Methods of program calc-hash-over-blocks
#[derive(Debug, Encode, Decode)]
pub enum OverBlocksMethod {
    Start(Package),
    Refuel(PackageId),
}

#[derive(Clone, Debug, Encode, Decode)]
pub struct Calculators<ProgramId> {
    /// ProgramId of over blocks calculator
    pub over_blocks: ProgramId,
    /// ProgramId of one block calculator
    pub in_one_block: ProgramId,
}

pub struct GasMeter {
    /// Last gas available
    pub last_gas_available: u64,

    /// Max gas spent per calculation
    pub max_gas_spent: u64,
}

impl GasMeter {
    /// Check if the given `gas_available` can run a calculation
    pub fn spin(&mut self, gas_available: u64) -> bool {
        if gas_available < self.last_gas_available {
            let last_gas_spent = self.last_gas_available - gas_available;
            self.max_gas_spent = self.max_gas_spent.max(last_gas_spent);
        }

        if gas_available < self.max_gas_spent {
            return false;
        }

        self.last_gas_available = gas_available;
        true
    }
}

#[derive(Clone, Debug, Encode, Decode)]
pub struct Package {
    /// Id of this calculation.
    pub id: PackageId,
    /// Paths of the calculation.
    pub paths: Vec<[u8; 32]>,
    /// Expected result.
    pub expected: [u8; 32],
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
        *self.paths.last().expect("invalid route")
    }
}

/// Do a sha2 256-bit hash and return result.
pub fn sha2_256(data: &[u8]) -> [u8; 32] {
    let mut output = [0u8; 32];
    output.copy_from_slice(sha2::Sha256::digest(data).as_slice());
    output
}
