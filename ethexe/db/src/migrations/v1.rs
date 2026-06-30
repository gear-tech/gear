// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use gprimitives::H256;
use parity_scale_codec::Decode;

pub const VERSION: u32 = 1;

/// Frozen v1 on-disk layout of `MbMeta`. Lives here because it describes the
/// v1 database version; the v1 -> v2 migration decodes existing records with
/// it before re-encoding them in the v2 layout.
#[derive(Decode)]
pub struct MbMeta {
    pub computed: bool,
    pub last_advanced_eb: H256,
}
