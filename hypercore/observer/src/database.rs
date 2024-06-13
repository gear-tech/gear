use gprimitives::{ActorId, H256};
use std::collections::BTreeMap;

pub trait Database {
    fn get_block_map(&self, block_hash: H256) -> BTreeMap<ActorId, H256>;

    fn set_block_map(&self, block_hash: H256, map: BTreeMap<ActorId, H256>);

    fn get_parent_hash(&self, block_hash: H256) -> H256;

    fn set_parent_hash(&self, block_hash: H256, parent_hash: H256);
}
