//! Command `send`
use crate::{api::signer::Signer, result::Result, utils};
use structopt::StructOpt;

/// Sends a message to a program or to another account.
///
/// The origin must be Signed and the sender must have sufficient funds to pay
/// for `gas` and `value` (in case the latter is being transferred).
///
/// To avoid an undefined behavior a check is made that the destination address
/// is not a program in uninitialized state. If the opposite holds true,
/// the message is not enqueued for processing.
///
/// Parameters:
/// - `destination`: the message destination.
/// - `payload`: in case of a program destination, parameters of the `handle` function.
/// - `gas_limit`: maximum amount of gas the program can spend before it is halted.
/// - `value`: balance to be transferred to the program once it's been created.
///
/// Emits the following events:
/// - `DispatchMessageEnqueued(MessageInfo)` when dispatch message is placed in the queue.
#[derive(StructOpt, Debug)]
pub struct Send {
    /// Send to
    pub destination: String,
    /// Send payload
    #[structopt(default_value = "0x")]
    pub payload: String,
    /// Send gas limit
    #[structopt(default_value = "0")]
    pub gas_limit: u64,
    /// Send value
    #[structopt(default_value = "0")]
    pub value: u128,
}

impl Send {
    pub async fn exec(&self, signer: Signer) -> Result<()> {
        signer
            .send_message(
                utils::hex_to_hash(&self.destination)?.into(),
                utils::hex_to_vec(&self.payload)?,
                self.gas_limit,
                self.value,
            )
            .await?;

        Ok(())
    }
}
