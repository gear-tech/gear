// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_common::SimpleBlockData;
use tokio::sync::{Notify, RwLock};

/// Ethereum chain-head register shared between [`crate::MalachiteService`]
/// (writer) and the externalities (reader).
pub struct ChainHead {
    /// Latest observed EB.
    pub latest: RwLock<SimpleBlockData>,
    /// Latest fully synced EB — reference point for quarantine and tx checks.
    pub latest_synced: RwLock<SimpleBlockData>,
    /// Wakes the producer when a new EB is synced.
    pub notify: Notify,
}
