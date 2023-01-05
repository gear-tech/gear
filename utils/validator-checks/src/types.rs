//! Shared types
use gp::api::config::GearConfig;
use subxt::{blocks, OnlineClient};

/// Gear block.
pub type Block = blocks::Block<GearConfig, OnlineClient<GearConfig>>;

/// Wrapper type for validators.
pub struct Address(
    // Address of the validator.
    String,
);
