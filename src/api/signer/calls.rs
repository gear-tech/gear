//! gear api calls
use crate::api::{
    config::GearConfig,
    generated::api::{runtime_types::sp_runtime::DispatchError, Event},
    signer::Signer,
    types::InBlock,
};
use subxt::{PolkadotExtrinsicParams, SubmittableExtrinsic, TransactionStatus};

mod balances {
    use crate::api::{generated::api::balances::calls, signer::Signer, types::InBlock};

    impl Signer {
        /// `pallet_balances::transfer`
        pub async fn transfer(&self, params: calls::Transfer) -> InBlock<'_> {
            let ex = self.tx().balances().transfer(params.dest, params.value)?;

            self.ps(ex).await
        }
    }
}

mod gear {
    use crate::api::{generated::api::gear::calls, signer::Signer, types::InBlock};

    impl Signer {
        /// `pallet_gear::send_reply`
        pub async fn claim_value_from_mailbox(&self, params: calls::ClaimValue) -> InBlock<'_> {
            let ex = self.api.tx().gear().claim_value(params.message_id)?;

            self.ps(ex).await
        }

        /// `pallet_gear::send_reply`
        pub async fn send_reply(&self, params: calls::SendReply) -> InBlock<'_> {
            let ex = self.tx().gear().send_reply(
                params.reply_to_id,
                params.payload,
                params.gas_limit,
                params.value,
            )?;

            self.ps(ex).await
        }

        /// `pallet_gear::send_message`
        pub async fn send_message(&self, params: calls::SendMessage) -> InBlock<'_> {
            let ex = self.tx().gear().send_message(
                params.destination,
                params.payload,
                params.gas_limit,
                params.value,
            )?;

            self.ps(ex).await
        }

        /// `pallet_gear::submit_program`
        pub async fn submit_program(&self, params: calls::UploadProgram) -> InBlock<'_> {
            let ex = self.tx().gear().upload_program(
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
            let ex = self.tx().gear().upload_code(params.code)?;

            self.ps(ex).await
        }
    }
}

impl Signer {
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
        let before = self.balance().await?;
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
                        self.log_balance_spent(before).await?;
                        break Err(status.into());
                    }
                    TransactionStatus::FinalityTimeout(h) => {
                        log::info!("\tStatus: FinalityTimeout( {} )", h);
                        self.log_balance_spent(before).await?;
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

                        self.log_balance_spent(before).await?;
                        return Ok(b);
                    }
                    TransactionStatus::Usurped(h) => {
                        log::info!("\tStatus: Usurped( {} )", h);
                        self.log_balance_spent(before).await?;
                        break Err(status.into());
                    }
                    TransactionStatus::Dropped => {
                        log::info!("\tStatus: Dropped");
                        self.log_balance_spent(before).await?;
                        break Err(status.into());
                    }
                    TransactionStatus::Invalid => {
                        log::info!("\tStatus: Invalid");
                        self.log_balance_spent(before).await?;
                        break Err(status.into());
                    }
                }
            }
        }
    }
}
