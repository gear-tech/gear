//! Events api
use crate::{
    api::{
        config::GearConfig,
        generated::api::{system::Event as SystemEvent, Event},
        types::Events,
        Api,
    },
    result::{ClientError, Result},
};
use futures_util::StreamExt;
use subxt::{
    error::{DispatchError, Error},
    events::{EventDetails, Phase, StaticEvent},
    tx::{TxEvents, TxInBlock},
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

                    return Err(Error::from(DispatchError::decode_from(
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
            log::info!("\tWeight cost: {:?}", dispatch_info.weight);
        }

        Ok(())
    }

    /// Wait for GearEvent.
    pub async fn wait_for<E>(mut events: Events) -> Result<E>
    where
        E: StaticEvent,
    {
        while let Some(events) = events.next().await {
            if let Ok(Some(e)) = events?.find_first::<E>() {
                return Ok(e);
            }
        }

        Err(ClientError::EventNotFound.into())
    }
}
