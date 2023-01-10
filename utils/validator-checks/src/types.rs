//! Shared types
use gp::api::config::GearConfig;
use parity_scale_codec::{Decode, Encode};
use std::collections::HashMap;
use subxt::{blocks, ext::sp_runtime::AccountId32, OnlineClient};

/// Gear block.
pub type Block = blocks::Block<GearConfig, OnlineClient<GearConfig>>;

/// Unit type wrapper that represents a slot.
#[derive(Debug, Encode, Decode)]
pub struct Slot(pub u64);

/// Validators mapping.
pub struct Validators(HashMap<AccountId32, Vec<[u8; 4]>>);

impl Validators {
    /// Get all validators.
    pub fn validators(&self) -> Vec<AccountId32> {
        self.0.keys().map(|acc| acc.clone()).collect()
    }

    /// Mark the check has been validated.
    pub fn validated(&mut self, acc: &AccountId32, check: [u8; 4]) -> bool {
        if let Some(checks) = self.0.get_mut(&acc) {
            if checks.contains(&check) {
                return false;
            }

            checks.push(check);
            return true;
        }

        false
    }

    /// Returns all unvalidated checks and the missing validators.
    pub fn unvalidated(&self, checks: &[[u8; 4]]) -> Vec<([u8; 4], Vec<&AccountId32>)> {
        let mut res = vec![];
        for check in checks {
            let validators = self
                .0
                .iter()
                .filter(|(_, validated_checks)| !validated_checks.contains(&check))
                .map(|(acc, _)| acc)
                .collect();

            res.push((*check, validators));
        }

        res
    }

    /// Validate if the specified check has been passed.
    pub fn validate(&self, check: &[u8; 4]) -> bool {
        self.0.values().all(|checks| checks.contains(&check))
    }

    /// Validate if all checks have been passed.
    pub fn validate_all(&self, checks: &[[u8; 4]]) -> Vec<[u8; 4]> {
        let mut validated = vec![];
        for check in checks {
            if !self.validate(check) {
                break;
            } else {
                validated.push(*check)
            }
        }

        validated
    }
}

impl From<Vec<AccountId32>> for Validators {
    fn from(validators: Vec<AccountId32>) -> Self {
        let mut mapping = HashMap::new();
        let mut iter = validators.into_iter();

        while let Some(acc) = iter.next() {
            mapping.insert(acc, Default::default());
        }

        Self(mapping)
    }
}

/// Wrapper type for validators.
pub struct Address(
    // Address of the validator.
    String,
);

impl From<String> for Address {
    fn from(s: String) -> Self {
        Self(s)
    }
}
