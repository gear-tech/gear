//! Command `reply`
use crate::{api::signer::Signer, result::Result, utils};
use clap::Parser;

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
#[derive(Parser, Debug)]
pub struct Reply {
    /// Reply to
    reply_to_id: String,
    /// Reply payload
    #[clap(default_value = "0x")]
    payload: String,
    /// Reply gas limit
    #[clap(default_value = "0")]
    gas_limit: u64,
    /// Reply value
    #[clap(default_value = "0")]
    value: u128,
}

impl Reply {
    pub async fn exec(&self, signer: Signer) -> Result<()> {
        let reply_to_id = utils::hex_to_hash(&self.reply_to_id)?.into();

        signer
            .send_reply(
                reply_to_id,
                utils::hex_to_vec(&self.payload)?,
                self.gas_limit,
                self.value,
            )
            .await?;

        Ok(())
    }
}
