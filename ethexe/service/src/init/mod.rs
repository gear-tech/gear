use gsigner::Address;

pub use version1::*;

mod version1;

pub const DB_VERSION_0: u32 = 0;
pub const DB_VERSION_1: u32 = 1;

pub struct InitConfig {
    pub ethereum_rpc: String,
    pub router_address: Address,
    pub slot_duration_secs: u64,
}
