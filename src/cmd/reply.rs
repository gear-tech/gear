//! Command `reply`
use crate::{
    api::{
        generated::api::{
            gear::{calls::SendReply, Event as GearEvent},
            runtime_types::gear_core::ids::MessageId,
        },
        Api,
    },
    result::Result,
};
use structopt::StructOpt;

/// Sends a reply message.
///
/// The origin must be Signed and the sender must have sufficient funds to pay
/// for `gas` and `value` (in case the latter is being transferred).
///
/// Parameters:
/// - `reply_to_id`: the original message id.
/// - `payload`: data expected by the original sender.
/// - `gas_limit`: maximum amount of gas the program can spend before it is halted.
/// - `value`: balance to be transferred to the program once it's been created.
///
/// - `DispatchMessageEnqueued(H256)` when dispatch message is placed in the queue.
#[derive(StructOpt, Debug)]
pub struct Reply {
    /// Reply to
    reply_to_id: String,
    /// Reply payload
    #[structopt(default_value = "0x")]
    payload: String,
    /// Reply gas limit
    #[structopt(default_value = "0")]
    gas_limit: u64,
    /// Reply value
    #[structopt(default_value = "0")]
    value: u128,
}

impl Reply {
    pub async fn exec(&self, api: Api) -> Result<()> {
        let events = api.events().await?;
        let r = tokio::try_join!(
            self.send_reply(&api),
            Api::wait_for(events, |event| {
                matches!(event, GearEvent::MessagesDispatched { .. })
            })
        );

        r?;

        Ok(())
    }

    async fn send_reply(&self, api: &Api) -> Result<()> {
        let mut reply_to_id = [0; 32];
        reply_to_id
            .copy_from_slice(hex::decode(self.reply_to_id.trim_start_matches("0x"))?.as_ref());

        api.send_reply(SendReply {
            reply_to_id: MessageId(reply_to_id),
            payload: hex::decode(self.payload.trim_start_matches("0x"))?,
            gas_limit: self.gas_limit,
            value: self.value,
        })
        .await?;

        Ok(())
    }
}
