//! gear api calls
use crate::api::{
    config::GearConfig,
    generated::api::{runtime_types::sp_runtime::DispatchError, Event},
    signer::Signer,
    types::InBlock,
};
use anyhow::anyhow;
use std::fmt::Display;
use subxt::{PolkadotExtrinsicParams, SubmittableExtrinsic, TransactionStatus};

mod balances {
    use crate::api::{signer::Signer, types::InBlock};
    use subxt::sp_runtime::AccountId32;

    impl Signer {
        /// `pallet_balances::transfer`
        pub async fn transfer(
            &self,
            destination: impl Into<AccountId32>,
            value: u128,
        ) -> InBlock<'_> {
            let ex = self
                .tx()
                .balances()
                .transfer(destination.into().into(), value)?;

            self.process(ex).await
        }
    }
}

mod gear {
    use crate::api::{signer::Signer, types::InBlock};
    use gear_core::ids::{CodeId, MessageId, ProgramId};

    impl Signer {
        /// `pallet_gear::create_program`
        pub async fn create_program(
            &self,
            code_id: CodeId,
            salt: Vec<u8>,
            payload: Vec<u8>,
            gas_limit: u64,
            value: u128,
        ) -> InBlock<'_> {
            let ex =
                self.tx()
                    .gear()
                    .create_program(code_id.into(), salt, payload, gas_limit, value)?;

            self.process(ex).await
        }

        /// `pallet_gear::claim_value`
        pub async fn claim_value(&self, message_id: MessageId) -> InBlock<'_> {
            let ex = self.tx().gear().claim_value(message_id.into())?;

            self.process(ex).await
        }

        /// `pallet_gear::reset`
        pub async fn reset(&self) -> InBlock<'_> {
            let ex = self.tx().gear().reset()?;

            self.process(ex).await
        }

        /// `pallet_gear::send_message`
        pub async fn send_message(
            &self,
            destination: ProgramId,
            payload: Vec<u8>,
            gas_limit: u64,
            value: u128,
        ) -> InBlock<'_> {
            let ex =
                self.tx()
                    .gear()
                    .send_message(destination.into(), payload, gas_limit, value)?;

            self.process(ex).await
        }

        /// `pallet_gear::send_reply`
        pub async fn send_reply(
            &self,
            reply_to_id: MessageId,
            payload: Vec<u8>,
            gas_limit: u64,
            value: u128,
        ) -> InBlock<'_> {
            let ex = self
                .tx()
                .gear()
                .send_reply(reply_to_id.into(), payload, gas_limit, value)?;

            self.process(ex).await
        }

        /// `pallet_gear::upload_code`
        pub async fn upload_code(&self, code: Vec<u8>) -> InBlock<'_> {
            let ex = self.tx().gear().upload_code(code)?;

            self.process(ex).await
        }

        /// `pallet_gear::upload_program`
        pub async fn upload_program(
            &self,
            code: Vec<u8>,
            salt: Vec<u8>,
            payload: Vec<u8>,
            gas_limit: u64,
            value: u128,
        ) -> InBlock<'_> {
            let ex = self
                .tx()
                .gear()
                .upload_program(code, salt, payload, gas_limit, value)?;

            self.process(ex).await
        }
    }
}

type Extrinsic<'client, Call, Config> = SubmittableExtrinsic<
    'client,
    Config,
    PolkadotExtrinsicParams<Config>,
    Call,
    DispatchError,
    Event,
>;

impl Signer {
    /// Propagates log::info for given status.
    pub(crate) fn log_status<Config>(
        &self,
        status: &TransactionStatus<Config, DispatchError, Event>,
    ) where
        Config: subxt::Config,
        Config::Hash: Display,
    {
        match status {
            TransactionStatus::Future => log::info!("\tStatus: Future"),
            TransactionStatus::Ready => log::info!("\tStatus: Ready"),
            TransactionStatus::Broadcast(v) => log::info!("\tStatus: Broadcast( {v:?} )"),
            TransactionStatus::InBlock(b) => log::info!(
                "\tStatus: InBlock( block hash: {}, extrinsic hash: {} )",
                b.block_hash(),
                b.extrinsic_hash()
            ),
            TransactionStatus::Retracted(h) => log::info!("\tStatus: Retracted( {h} )"),
            TransactionStatus::FinalityTimeout(h) => log::info!("\tStatus: FinalityTimeout( {h} )"),
            TransactionStatus::Finalized(b) => log::info!(
                "\tStatus: Finalized( block hash: {}, extrinsic hash: {} )",
                b.block_hash(),
                b.extrinsic_hash()
            ),
            TransactionStatus::Usurped(h) => log::info!("\tStatus: Usurped( {h} )"),
            TransactionStatus::Dropped => log::info!("\tStatus: Dropped"),
            TransactionStatus::Invalid => log::info!("\tStatus: Invalid"),
        }
    }
    /// listen transaction process and print logs
    pub async fn process<'client, Call>(
        &'client self,
        tx: Extrinsic<'client, Call, GearConfig>,
    ) -> InBlock<'client>
    where
        Call: subxt::Call + Send + Sync,
    {
        use TransactionStatus::*;

        let before = self.balance().await?;
        let mut process = tx.sign_and_submit_then_watch_default(&self.signer).await?;

        log::info!("Submitted extrinsic {}::{}", Call::PALLET, Call::FUNCTION);

        while let Some(status) = process.next_item().await {
            let status = status?;
            self.log_status(&status);
            // TODO [SAB] Remove
            println!("[SAB] Received status {status:?}");
            match status {
                Future | Ready | Broadcast(_) | InBlock(_) => (),
                Dropped | Invalid | Usurped(_) | FinalityTimeout(_) | Retracted(_) => {
                    self.log_balance_spent(before).await?;
                    return Err(status.into());
                }
                Finalized(b) => {
                    log::info!(
                        "Successfully submitted call {}::{} {} at {}!",
                        Call::PALLET,
                        Call::FUNCTION,
                        b.extrinsic_hash(),
                        b.block_hash()
                    );

                    self.log_balance_spent(before).await?;
                    return Ok(b);
                }
            }
        }

        Err(anyhow!("Transaction wasn't found").into())
    }
}
