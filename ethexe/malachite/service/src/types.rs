use ethexe_common::SimpleBlockData;
use tokio::sync::{Notify, RwLock};

pub struct ChainHead {
    pub latest: RwLock<SimpleBlockData>,
    pub latest_synced: RwLock<SimpleBlockData>,
    pub notify: Notify,
}
