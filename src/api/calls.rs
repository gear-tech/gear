//! gear api calls
use crate::{
    api::{
        config::GearConfig,
        generated::api::{runtime_types::sp_runtime::DispatchError, Event},
        Api,
    },
    Result,
};
use subxt::{
    sp_core::crypto::Ss58Codec, PolkadotExtrinsicParams, SubmittableExtrinsic, TransactionInBlock,
    TransactionStatus,
};

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
        pub async fn claim_value_from_mailbox(
            &self,
            params: calls::ClaimValueFromMailbox,
        ) -> InBlock<'_> {
            let ex = self
                .runtime
                .tx()
                .gear()
                .claim_value_from_mailbox(params.message_id)?;

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
        pub async fn submit_program(&self, params: calls::SubmitProgram) -> InBlock<'_> {
            let ex = self.runtime.tx().gear().submit_program(
                params.code,
                params.salt,
                params.init_payload,
                params.gas_limit,
                params.value,
            )?;

            self.ps(ex).await
        }

        /// `pallet_gear::submit_code`
        pub async fn submit_code(&self, params: calls::SubmitCode) -> InBlock<'_> {
            let ex = self.runtime.tx().gear().submit_code(params.code)?;

            self.ps(ex).await
        }
    }
}

impl Api {
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
        let signer_address = self.signer.account_id().to_ss58check();
        let mut balance = self.get_balance(&signer_address).await?;
        let mut process = tx.sign_and_submit_then_watch_default(&self.signer).await?;
        println!("Submited extrinsic {}::{}", Call::PALLET, Call::FUNCTION);

        loop {
            if let Some(status) = process.next_item().await {
                let status = status?;
                match status {
                    TransactionStatus::Future => println!("\tStatus: Future"),
                    TransactionStatus::Ready => println!("\tStatus: Ready"),
                    TransactionStatus::Broadcast(v) => println!("\tStatus: Broadcast( {:?} )", v),
                    TransactionStatus::InBlock(b) => println!(
                        "\tStatus: InBlock( block_hash: {}, extrinsic_hash: {} )",
                        b.block_hash(),
                        b.extrinsic_hash()
                    ),
                    TransactionStatus::Retracted(h) => {
                        println!("\tStatus: Retracted( {} )", h);
                        break Err(status.into());
                    }
                    TransactionStatus::FinalityTimeout(h) => {
                        println!("\tStatus: FinalityTimeout( {} )", h);
                        break Err(status.into());
                    }
                    TransactionStatus::Finalized(b) => {
                        println!(
                            "\tStatus: Finalized( block_hash: {}, extrinsic_hash: {} )",
                            b.block_hash(),
                            b.extrinsic_hash()
                        );

                        println!(
                            "Successfully submited call {}::{} {} at {}!",
                            Call::PALLET,
                            Call::FUNCTION,
                            b.extrinsic_hash(),
                            b.block_hash()
                        );

                        self.capture_dispatch_info(&b).await?;
                        balance = balance.saturating_sub(self.get_balance(&signer_address).await?);

                        println!("\tBalance spent: {balance}");
                        return Ok(b);
                    }
                    TransactionStatus::Usurped(h) => {
                        println!("\tStatus: Usurped( {} )", h);
                        break Err(status.into());
                    }
                    TransactionStatus::Dropped => {
                        println!("\tStatus: Dropped");
                        break Err(status.into());
                    }
                    TransactionStatus::Invalid => {
                        println!("\tStatus: Invalid");
                        break Err(status.into());
                    }
                }
            }
        }
    }
}
