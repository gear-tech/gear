//! Command `claim`
use crate::{
    api::{
        generated::api::{gear::calls::ClaimValue, runtime_types::gear_core::ids::MessageId},
        signer::Signer,
    },
    result::Result,
};
use structopt::StructOpt;

/// Claim value from mailbox.
#[derive(StructOpt, Debug)]
pub struct Claim {
    /// Claim value from.
    message_id: String,
}

impl Claim {
    pub async fn exec(&self, signer: Signer) -> Result<()> {
        let mut message_id = [0; 32];

        message_id.copy_from_slice(hex::decode(self.message_id.trim_start_matches("0x"))?.as_ref());
        signer
            .claim_value_from_mailbox(ClaimValue {
                message_id: MessageId(message_id),
            })
            .await?;

        Ok(())
    }
}
