//! gear api calls
use crate::{
    api::{
        config::GearConfig,
        generated::api::{gear, DispatchError, Event},
        Api,
    },
    Result,
};
use subxt::{sp_core::H256, PolkadotExtrinsicParams, SubmittableExtrinsic, TransactionStatus};

impl Api {
    /// pallet gear extrinsic
    ///
    /// gear submit_program
    pub async fn submit_program(&self, params: gear::calls::SubmitProgram) -> Result<H256> {
        let process = self.api.tx().gear().submit_program(
            params.code,
            params.salt,
            params.init_payload,
            params.gas_limit,
            params.value,
        )?;

        self.ps(process).await
    }

    /// listen transaction process and print logs
    pub async fn ps<'client, Call>(
        &self,
        tx: SubmittableExtrinsic<
            'client,
            GearConfig,
            PolkadotExtrinsicParams<GearConfig>,
            Call,
            DispatchError,
            Event,
        >,
    ) -> Result<H256>
    where
        Call: subxt::Call + Send + Sync,
    {
        let mut process = tx.sign_and_submit_then_watch_default(&self.signer).await?;
        println!("Submited call {}::{}", Call::PALLET, Call::FUNCTION);

        loop {
            if let Some(status) = process.next_item().await {
                match status? {
                    TransactionStatus::Future => println!("\tStatus: Future"),
                    TransactionStatus::Ready => println!("\tStatus: Ready"),
                    TransactionStatus::Broadcast(v) => println!("\tStatus: Broadcast( {:?} )", v),
                    TransactionStatus::InBlock(b) => println!(
                        "\tStatus: InBlock( block_hash: {}, extrinsic_hash: {} )",
                        b.block_hash(),
                        b.extrinsic_hash()
                    ),
                    TransactionStatus::Retracted(h) => println!("\tStatus: Retracted( {} )", h),
                    TransactionStatus::FinalityTimeout(h) => {
                        println!("\tStatus: FinalityTimeout( {} )", h)
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
                        return Ok(b.extrinsic_hash());
                    }
                    TransactionStatus::Usurped(h) => println!("\tStatus: Usurped( {} )", h),
                    TransactionStatus::Dropped => println!("\tStatus: Dropped"),
                    TransactionStatus::Invalid => println!("\tStatus: Invalid"),
                }
            }
        }
    }
}
