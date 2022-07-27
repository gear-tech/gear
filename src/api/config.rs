use subxt::{sp_core, sp_runtime, Config};

/// gear config
///
/// see https://github.com/gear-tech/gear/blob/f48450dd9bad2efb9cb3fb13353464ca73e7b7f9/runtime/src/lib.rs#L183
#[derive(Clone, Debug)]
pub struct GearConfig;

impl Config for GearConfig {
    type Index = u32;
    type BlockNumber = u32;
    type Hash = sp_core::H256;
    type Hashing = sp_runtime::traits::BlakeTwo256;
    type AccountId = sp_runtime::AccountId32;
    type Address = sp_runtime::MultiAddress<Self::AccountId, ()>;
    type Header = sp_runtime::generic::Header<Self::BlockNumber, sp_runtime::traits::BlakeTwo256>;
    type Signature = sp_runtime::MultiSignature;
    type Extrinsic = sp_runtime::OpaqueExtrinsic;
}
