// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Events api

use crate::{
    Api, AsGear,
    config::GearConfig,
    gear::{self},
    result::Result,
};
use subxt::{OnlineClient, blocks::ExtrinsicEvents as TxEvents, tx::TxInBlock};

impl Api {
    /// Capture the dispatch info of any extrinsic and display the weight spent
    pub async fn capture_dispatch_info(
        &self,
        tx: &TxInBlock<GearConfig, OnlineClient<GearConfig>>,
    ) -> Result<TxEvents<GearConfig>> {
        let events = tx.fetch_events().await?;

        for ev in events.iter() {
            if let gear::Event::System(system_event) = ev?.as_gear()? {
                let extrinsic_result = match system_event {
                    gear::system::Event::ExtrinsicFailed {
                        dispatch_error,
                        dispatch_info,
                    } => Some((dispatch_info, Err(self.decode_error(dispatch_error)))),
                    gear::system::Event::ExtrinsicSuccess { dispatch_info } => {
                        Some((dispatch_info, Ok(())))
                    }
                    _ => None,
                };

                if let Some((dispatch_info, result)) = extrinsic_result {
                    log::info!("	Weight cost: {:?}", dispatch_info.weight);
                    result?;
                    break;
                }
            }
        }

        Ok(events)
    }
}
