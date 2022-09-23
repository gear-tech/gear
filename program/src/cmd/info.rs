//! command `info`
use crate::{
    api::{
        generated::api::runtime_types::{
            gear_common::storage::primitives::Interval,
            gear_core::message::{common::ReplyDetails, stored::StoredMessage},
        },
        signer::Signer,
    },
    result::{Error, Result},
};
use std::fmt;
use structopt::StructOpt;
use subxt::{
    sp_core::{crypto::Ss58Codec, sr25519::Pair, Pair as PairT},
    sp_runtime::AccountId32,
};

#[derive(Debug, StructOpt)]
pub enum Action {
    Balance,
    Mailbox {
        /// The count of mails for fetching
        #[structopt(default_value = "10", short, long)]
        count: u32,
    },
}

/// Get account info from ss58address.
#[derive(Debug, StructOpt)]
pub struct Info {
    /// Info of this address, if none, will use the logged in account.
    pub address: Option<String>,

    /// Info of balance, mailbox, etc.
    #[structopt(subcommand)]
    pub action: Action,
}

impl Info {
    /// execute command transfer
    pub async fn exec(&self, signer: Signer) -> Result<()> {
        let mut address = self.address.clone().unwrap_or_else(|| signer.address());
        if address.starts_with("//") {
            address = Pair::from_string(&address, None)
                .expect("Parse development address failed")
                .public()
                .to_ss58check()
        }

        match self.action {
            Action::Balance => Self::balance(signer, &address).await,
            Action::Mailbox { count } => Self::mailbox(signer, &address, count).await,
        }
    }

    /// Get balance of address
    pub async fn balance(signer: Signer, address: &str) -> Result<()> {
        let info = signer.info(address).await?;

        println!("{info:#?}");

        Ok(())
    }

    /// Get mailbox of address
    pub async fn mailbox(signer: Signer, address: &str, count: u32) -> Result<()> {
        let mails = signer
            .mailbox(
                AccountId32::from_ss58check(address).map_err(|_| Error::InvalidPublic)?,
                count,
            )
            .await?;

        for t in mails.into_iter() {
            println!("{:#?}", Mail::from(t));
        }
        Ok(())
    }
}

struct Mail {
    message: StoredMessage,
    interval: Interval<u32>,
}

impl From<(StoredMessage, Interval<u32>)> for Mail {
    fn from(t: (StoredMessage, Interval<u32>)) -> Self {
        Self {
            message: t.0,
            interval: t.1,
        }
    }
}

impl fmt::Debug for Mail {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Mail")
            .field("id", &["0x", &hex::encode(self.message.id.0)].concat())
            .field(
                "source",
                &["0x", &hex::encode(self.message.source.0)].concat(),
            )
            .field(
                "destination",
                &["0x", &hex::encode(self.message.destination.0)].concat(),
            )
            .field(
                "payload",
                &["0x", &hex::encode(&self.message.payload)].concat(),
            )
            .field("value", &self.message.value)
            .field("reply", &self.message.reply.as_ref().map(DebugReplyDetails))
            .field("interval", &self.interval)
            .finish()
    }
}

struct DebugReplyDetails<'d>(pub &'d ReplyDetails);

impl<'d> fmt::Debug for DebugReplyDetails<'d> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("ReplyDetails")
            .field("reply_to", &hex::encode(self.0.reply_to.0))
            .field("exit_code", &self.0.exit_code.to_string())
            .finish()
    }
}
