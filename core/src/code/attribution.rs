use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

use primitive_types::H256;

/// Code attribution.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
pub struct CodeAttribution {
    /// Author of the code.
    pub author: H256,
    /// Block number when the code was uploaded.
    #[codec(compact)]
    pub block_number: u32,
}

impl CodeAttribution {
    /// Creates a new instance of the code attribution.
    pub fn new(author: H256, block_number: u32) -> Self {
        CodeAttribution {
            author,
            block_number,
        }
    }
}
