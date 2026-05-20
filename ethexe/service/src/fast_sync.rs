// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Fast synchronization stub — always fails so `--fast-sync` doesn't
//! silently degrade to a full-from-genesis catch-up.

use crate::Service;
use anyhow::{Result, bail};

// TODO: #5487 implement the actual fast-sync logic.
pub(crate) async fn sync(_service: &mut Service) -> Result<()> {
    bail!(
        "fast-sync is not implemented for the MB-driven recovery path yet; \
         start the node without --fast-sync (or omit `fast_sync = true` in config)"
    );
}
