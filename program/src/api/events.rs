//! Events api
use crate::{
    api::{
        config::GearConfig,
        generated::api::{
            gear::Event as GearEvent, system::events::ExtrinsicSuccess,
            system::Event as SystemEvent, DispatchError, Event,
        },
        types::Events,
        Api,
    },
    result::{ClientError, Result},
};
use futures_util::StreamExt;
use parity_scale_codec::Decode;
use subxt::{
    // HasModuleError, ModuleError, RuntimeError,
    error::ModuleError,
    events::{EventSubscription, StaticEvent},
    ext::sp_runtime::{generic::Header, traits::BlakeTwo256},
    rpc::Subscription,
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

        if let Ok(Some(success)) = events.find_first::<ExtrinsicSuccess>() {
            // let event = success.
        }

        // // Try to find any errors; return the first one we encounter.
        // for event in events.iter() {
        //     let ev = event?;
        //     if ev.pallet_name() == "System" && ev.variant_name() == "ExtrinsicFailed" {
        //         // // Self::capture_weight_info(event?.event);
        //         // let dispatch_error = DispatchError::decode(&mut &*ev.data)?;
        //         // if let Some(error_data) = dispatch_error.module_error_data() {
        //         //     // Error index is utilized as the first byte from the error array.
        //         //     let metadata = self.metadata();
        //         //     let details =
        //         //         metadata.error(error_data.pallet_index, error_data.error_index())?;
        //         //     return Err(subxt::Error::Module(ModuleError {
        //         //         pallet: details.pallet().to_string(),
        //         //         error: details.error().to_string(),
        //         //         description: details.description().to_vec(),
        //         //         error_data,
        //         //     })
        //         //     .into());
        //         // } else {
        //         //     return Err(subxt::Error::Runtime(dispatch_error).into());
        //         // }
        //     } else if ev.pallet_name() == "System" && ev.variant_name() == "ExtrinsicSuccess" {
        //         Self::capture_weight_info(ev);
        //
        //         break;
        //     }
        // }

        Ok(events)
    }

    /// Parse transaction fee from InBlockEvents
    pub fn capture_weight_info(event: Event) {
        if let Event::System(SystemEvent::ExtrinsicSuccess { dispatch_info })
        | Event::System(SystemEvent::ExtrinsicFailed { dispatch_info, .. }) = event
        {
            log::info!("\tWeight cost: {:?}", dispatch_info.weight);
        }
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
