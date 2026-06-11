// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_common::SimpleBlockData;
use tokio::sync::{Notify, RwLock};

pub struct ChainHead {
    pub latest: RwLock<SimpleBlockData>,
    pub latest_synced: RwLock<SimpleBlockData>,
    pub notify: Notify,
}
