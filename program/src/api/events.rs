//! Events api
use crate::{
    api::{
        config::GearConfig,
        generated::api::{system::Event as SystemEvent, Event},
        Api,
    },
    result::{ClientError, Result},
};
use subxt::{
    blocks::ExtrinsicEvents as TxEvents,
    error::{DispatchError, Error},
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

        Err(ClientError::EventNotFound.into())
    }
}
