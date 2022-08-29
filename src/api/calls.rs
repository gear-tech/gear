//! gear api calls
use crate::{
    api::{
        config::GearConfig,
        generated::api::{runtime_types::sp_runtime::DispatchError, Event},
        Api,
    },
    result::Result,
};
use subxt::{PolkadotExtrinsicParams, SubmittableExtrinsic, TransactionInBlock, TransactionStatus};

type InBlock<'i> = Result<TransactionInBlock<'i, GearConfig, DispatchError, Event>>;

mod balances {
    use crate::api::{calls::InBlock, generated::api::balances::calls, Api};

    impl Api {
        /// `pallet_balances::transfer`
        pub async fn transfer(&self, params: calls::Transfer) -> InBlock<'_> {
            let ex = self
                .runtime
                .tx()
                .balances()
                .transfer(params.dest, params.value)?;

            self.ps(ex).await
        }
    }
}

mod gear {
    use crate::api::{calls::InBlock, generated::api::gear::calls, Api};

    impl Api {
        /// `pallet_gear::send_reply`
        pub async fn claim_value_from_mailbox(&self, params: calls::ClaimValue) -> InBlock<'_> {
            let ex = self.runtime.tx().gear().claim_value(params.message_id)?;

            self.ps(ex).await
        }

        /// `pallet_gear::send_reply`
        pub async fn send_reply(&self, params: calls::SendReply) -> InBlock<'_> {
            let ex = self.runtime.tx().gear().send_reply(
                params.reply_to_id,
                params.payload,
                params.gas_limit,
                params.value,
            )?;

            self.ps(ex).await
        }

        /// `pallet_gear::send_message`
        pub async fn send_message(&self, params: calls::SendMessage) -> InBlock<'_> {
            let ex = self.runtime.tx().gear().send_message(
                params.destination,
                params.payload,
                params.gas_limit,
                params.value,
            )?;

            self.ps(ex).await
        }

        /// `pallet_gear::submit_program`
        pub async fn submit_program(&self, params: calls::UploadProgram) -> InBlock<'_> {
            let ex = self.runtime.tx().gear().upload_program(
                params.code,
                params.salt,
                params.init_payload,
                params.gas_limit,
                params.value,
            )?;

            self.ps(ex).await
        }

        /// `pallet_gear::upload_code`
        pub async fn upload_code(&self, params: calls::UploadCode) -> InBlock<'_> {
            let ex = self.runtime.tx().gear().upload_code(params.code)?;

            self.ps(ex).await
        }
    }
}

impl Api {
    /// Comparing the latest balance with the balance
    /// recorded in the tracker and then log
    pub async fn log_balance_spent(&self) -> Result<()> {
        let balance_before = *self.balance.borrow();
        let balance_after = self.update_balance().await?;

        log::info!(
            "\tBalance spent: {}",
            balance_before.saturating_sub(balance_after)
        );
        Ok(())
    }

    /// listen transaction process and print logs
    pub async fn ps<'client, Call>(
        &'client self,
        tx: SubmittableExtrinsic<
            'client,
            GearConfig,
            PolkadotExtrinsicParams<GearConfig>,
            Call,
            DispatchError,
            Event,
        >,
    ) -> InBlock<'client>
    where
        Call: subxt::Call + Send + Sync,
    {
        self.update_balance().await?;
        let mut process = tx.sign_and_submit_then_watch_default(&self.signer).await?;
        log::info!("Submited extrinsic {}::{}", Call::PALLET, Call::FUNCTION);

        loop {
            if let Some(status) = process.next_item().await {
                let status = status?;
                match status {
                    TransactionStatus::Future => log::info!("\tStatus: Future"),
                    TransactionStatus::Ready => log::info!("\tStatus: Ready"),
                    TransactionStatus::Broadcast(v) => log::info!("\tStatus: Broadcast( {:?} )", v),
                    TransactionStatus::InBlock(b) => log::info!(
                        "\tStatus: InBlock( block_hash: {}, extrinsic_hash: {} )",
                        b.block_hash(),
                        b.extrinsic_hash()
                    ),
                    TransactionStatus::Retracted(h) => {
                        log::info!("\tStatus: Retracted( {} )", h);
                        break Err(status.into());
                    }
                    TransactionStatus::FinalityTimeout(h) => {
                        log::info!("\tStatus: FinalityTimeout( {} )", h);
                        break Err(status.into());
                    }
                    TransactionStatus::Finalized(b) => {
                        log::info!(
                            "\tStatus: Finalized( block_hash: {}, extrinsic_hash: {} )",
                            b.block_hash(),
                            b.extrinsic_hash()
                        );

                        log::info!(
                            "Successfully submited call {}::{} {} at {}!",
                            Call::PALLET,
                            Call::FUNCTION,
                            b.extrinsic_hash(),
                            b.block_hash()
                        );

                        self.capture_dispatch_info(&b).await?;
                        return Ok(b);
                    }
                    TransactionStatus::Usurped(h) => {
                        log::info!("\tStatus: Usurped( {} )", h);
                        break Err(status.into());
                    }
                    TransactionStatus::Dropped => {
                        log::info!("\tStatus: Dropped");
                        break Err(status.into());
                    }
                    TransactionStatus::Invalid => {
                        log::info!("\tStatus: Invalid");
                        break Err(status.into());
                    }
                }
            }
        }
    }
}
