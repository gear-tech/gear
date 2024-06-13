use gprimitives::{ActorId, H256};
use std::collections::BTreeMap;

pub trait Database {
    fn get_program_state_hashes(&self, block_hash: H256) -> Option<BTreeMap<ActorId, H256>>;

    fn set_program_state_hashes(&self, block_hash: H256, map: BTreeMap<ActorId, H256>);

    fn get_block_parent_hash(&self, block_hash: H256) -> Option<H256>;

    fn set_block_parent_hash(&self, block_hash: H256, parent_hash: H256);
}

impl Database for hypercore_db::Database {
    fn get_program_state_hashes(&self, block_hash: H256) -> Option<BTreeMap<ActorId, H256>> {
        self.get_block_map(block_hash)
    }

    fn set_program_state_hashes(&self, block_hash: H256, map: BTreeMap<ActorId, H256>) {
        self.set_block_map(block_hash, map)
    }

    fn get_block_parent_hash(&self, block_hash: H256) -> Option<H256> {
        self.get_parent_hash(block_hash)
    }

    fn set_block_parent_hash(&self, block_hash: H256, parent_hash: H256) {
        self.set_parent_hash(block_hash, parent_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database() {
        let db = hypercore_db::MemDb::default();
        let database = hypercore_db::Database::from_one(&db);

        let block_hash = H256::zero();
        let parent_hash = H256::zero();
        let map: BTreeMap<ActorId, H256> = [(ActorId::zero(), H256::zero())].into();

        database.set_program_state_hashes(block_hash, map.clone());
        assert_eq!(database.get_program_state_hashes(block_hash), Some(map));

        database.set_block_parent_hash(block_hash, parent_hash);
        assert_eq!(
            database.get_block_parent_hash(block_hash),
            Some(parent_hash)
        );
    }
}
