// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! This module provides shorthands for retrieving some frequently used constants.

use crate::{Api, Result, gear::constants};

impl Api {
    /// Retrieves block gas limit value.
    pub fn block_gas_limit(&self) -> Result<u64> {
        Ok(self
            .constants()
            .at(&constants().gear_gas().block_gas_limit())?)
    }

    /// Retrieves expected block time value.
    pub fn expected_block_time(&self) -> Result<u64> {
        Ok(self
            .constants()
            .at(&constants().babe().expected_block_time())?)
    }
}
