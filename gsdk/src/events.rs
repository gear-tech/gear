// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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
    config::GearConfig,
    metadata::{system::Event as SystemEvent, Event},
    result::{Error, Result},
    Api,
};
use subxt::{
    blocks::ExtrinsicEvents as TxEvents,
    error::{DispatchError, Error as SubxtError},
    events::{EventDetails, Phase},
    tx::TxInBlock,
    OnlineClient,
};

impl Api {
    /// Capture the dispatch info of any extrinsic and display the weight spent
    pub async fn capture_dispatch_info(
        &self,
        tx: &TxInBlock<GearConfig, OnlineClient<GearConfig>>,
    ) -> Result<TxEvents<GearConfig>> {
        let events = tx.fetch_events().await?;

        for ev in events.iter() {
            let ev = ev?;
            if ev.pallet_name() == "System" {
                if ev.variant_name() == "ExtrinsicFailed" {
                    Self::capture_weight_info(&ev)?;

                    return Err(SubxtError::from(DispatchError::decode_from(
                        ev.field_bytes(),
                        &self.metadata(),
                    ))
                    .into());
                }

                if ev.variant_name() == "ExtrinsicSuccess" {
                    Self::capture_weight_info(&ev)?;
                    break;
                }
            }
        }

        Ok(events)
    }

    /// Parse transaction fee from InBlockEvents
    pub fn capture_weight_info(details: &EventDetails) -> Result<()> {
        let event: Event = details.as_root_event::<(Phase, Event)>()?.1;

        if let Event::System(SystemEvent::ExtrinsicSuccess { dispatch_info })
        | Event::System(SystemEvent::ExtrinsicFailed { dispatch_info, .. }) = event
        {
            log::info!("	Weight cost: {:?}", dispatch_info.weight);
        }

        Err(Error::EventNotFound)
    }
}
