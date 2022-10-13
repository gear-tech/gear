//! Events api
use crate::{
    api::{
        config::GearConfig,
        generated::api::{
            gear::Event as GearEvent, system::Event as SystemEvent, DispatchError, Event,
        },
        Api,
    },
    result::Result,
};
use futures_util::StreamExt;
use subxt::{
    codec::Decode,
    events::EventSubscription,
    rpc::Subscription,
    sp_runtime::{generic::Header, traits::BlakeTwo256},
    HasModuleError, ModuleError, RuntimeError, TransactionEvents, TransactionInBlock,
};

/// Generic events
pub type Events<'a> =
    EventSubscription<'a, Subscription<Header<u32, BlakeTwo256>>, GearConfig, Event>;

/// Transaction events
#[allow(unused)]
pub type InBlockEvents = TransactionEvents<GearConfig, Event>;

impl Api {
    /// Capture the dispatch info of any extrinsic and display the weight spent
    pub async fn capture_dispatch_info<'e>(
        &'e self,
        tx: &TransactionInBlock<'e, GearConfig, DispatchError, Event>,
    ) -> Result<InBlockEvents> {
        let events = tx.fetch_events().await?;

        // Try to find any errors; return the first one we encounter.
        for (raw, event) in events.iter_raw().zip(events.iter()) {
            let ev = raw?;
            if &ev.pallet == "System" && &ev.variant == "ExtrinsicFailed" {
                Self::capture_weight_info(event?.event);
                let dispatch_error = DispatchError::decode(&mut &*ev.data)?;
                if let Some(error_data) = dispatch_error.module_error_data() {
                    // Error index is utilized as the first byte from the error array.
                    let locked_metadata = self.metadata();
                    let metadata = locked_metadata.read();
                    let details =
                        metadata.error(error_data.pallet_index, error_data.error_index())?;
                    return Err(subxt::Error::Module(ModuleError {
                        pallet: details.pallet().to_string(),
                        error: details.error().to_string(),
                        description: details.description().to_vec(),
                        error_data,
                    })
                    .into());
                } else {
                    return Err(subxt::Error::Runtime(RuntimeError(dispatch_error)).into());
                }
            } else if &ev.pallet == "System" && &ev.variant == "ExtrinsicSuccess" {
                Self::capture_weight_info(event?.event);

                break;
            }
        }

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
    pub async fn wait_for(mut events: Events<'_>, wait: fn(GearEvent) -> bool) -> Result<()> {
        while let Some(events) = events.next().await {
            for maybe_event in events?.iter() {
                let event = maybe_event?.event;

                // Exit when extrinsic failed.
                //
                // # Safety
                //
                // The error message will be panicked in another thread.
                if let Event::System(SystemEvent::ExtrinsicFailed { .. }) = event {
                    return Ok(());
                }

                // Exit when success or failure.
                if let Event::Gear(e) = event {
                    log::info!("\t{e:?}");

                    if wait(e) {
                        return Ok(());
                    }
                }
            }
        }

        Ok(())
    }
}
