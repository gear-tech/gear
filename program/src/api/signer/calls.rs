//! gear api calls
use crate::api::{
    signer::Signer,
    types::{InBlock, TxStatus},
};
use anyhow::anyhow;
use subxt::{ext::codec::Encode, tx::StaticTxPayload};

mod balances {
    use crate::api::{generated::api::tx, signer::Signer, types::InBlock};
    use subxt::ext::sp_runtime::AccountId32;

    impl Signer {
        /// `pallet_balances::transfer`
        pub async fn transfer(&self, destination: impl Into<AccountId32>, value: u128) -> InBlock {
            let ex = tx().balances().transfer(destination.into().into(), value);
            self.process(ex, "balances", "transfer").await
        }
    }
}

mod gear {
    use crate::api::{generated::api::tx, signer::Signer, types::InBlock};
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
        ) -> InBlock {
            let ex = tx()
                .gear()
                .create_program(code_id.into(), salt, payload, gas_limit, value);

            self.process(ex, "gear", "create_program").await
        }

        /// `pallet_gear::claim_value`
        pub async fn claim_value(&self, message_id: MessageId) -> InBlock {
            let ex = tx().gear().claim_value(message_id.into());

            self.process(ex, "gear", "claim_value").await
        }

        /// `pallet_gear::reset`
        pub async fn reset(&self) -> InBlock {
            let ex = tx().gear().reset();

            self.process(ex, "gear", "reset").await
        }

        /// `pallet_gear::send_message`
        pub async fn send_message(
            &self,
            destination: ProgramId,
            payload: Vec<u8>,
            gas_limit: u64,
            value: u128,
        ) -> InBlock {
            let ex = tx()
                .gear()
                .send_message(destination.into(), payload, gas_limit, value);

            self.process(ex, "gear", "send_message").await
        }

        /// `pallet_gear::send_reply`
        pub async fn send_reply(
            &self,
            reply_to_id: MessageId,
            payload: Vec<u8>,
            gas_limit: u64,
            value: u128,
        ) -> InBlock {
            let ex = tx()
                .gear()
                .send_reply(reply_to_id.into(), payload, gas_limit, value);

            self.process(ex, "gear", "send_reply").await
        }

        /// `pallet_gear::upload_code`
        pub async fn upload_code(&self, code: Vec<u8>) -> InBlock {
            let ex = tx().gear().upload_code(code);

            self.process(ex, "gear", "upload_code").await
        }

        /// `pallet_gear::upload_program`
        pub async fn upload_program(
            &self,
            code: Vec<u8>,
            salt: Vec<u8>,
            payload: Vec<u8>,
            gas_limit: u64,
            value: u128,
        ) -> InBlock {
            let ex = tx()
                .gear()
                .upload_program(code, salt, payload, gas_limit, value);

            self.process(ex, "gear", "upload_program").await
        }
    }
}

impl Signer {
    /// Propagates log::info for given status.
    pub(crate) fn log_status(&self, status: &TxStatus) {
        match status {
            TxStatus::Future => log::info!("\tStatus: Future"),
            TxStatus::Ready => log::info!("\tStatus: Ready"),
            TxStatus::Broadcast(v) => log::info!("\tStatus: Broadcast( {v:?} )"),
            TxStatus::InBlock(b) => log::info!(
                "\tStatus: InBlock( block hash: {}, extrinsic hash: {} )",
                b.block_hash(),
                b.extrinsic_hash()
            ),
            TxStatus::Retracted(h) => log::info!("\tStatus: Retracted( {h} )"),
            TxStatus::FinalityTimeout(h) => log::info!("\tStatus: FinalityTimeout( {h} )"),
            TxStatus::Finalized(b) => log::info!(
                "\tStatus: Finalized( block hash: {}, extrinsic hash: {} )",
                b.block_hash(),
                b.extrinsic_hash()
            ),
            TxStatus::Usurped(h) => log::info!("\tStatus: Usurped( {h} )"),
            TxStatus::Dropped => log::info!("\tStatus: Dropped"),
            TxStatus::Invalid => log::info!("\tStatus: Invalid"),
        }
    }

    /// listen transaction process and print logs
    pub async fn process<CallData: Encode>(
        &self,
        tx: StaticTxPayload<CallData>,
        pallet: &str,
        name: &str,
    ) -> InBlock {
        use subxt::tx::TxStatus::*;

        let before = self.balance().await?;
        let mut process = self
            .api
            .tx()
            .sign_and_submit_then_watch_default(&tx, &self.signer)
            .await?;

        log::info!("Submitted extrinsic {}::{}", pallet, name);

        while let Some(status) = process.next_item().await {
            let status = status?;
            self.log_status(&status);
            match status {
                Future | Ready | Broadcast(_) | InBlock(_) => (),
                Dropped | Invalid | Usurped(_) | FinalityTimeout(_) | Retracted(_) => {
                    self.log_balance_spent(before).await?;
                    return Err(status.into());
                }
                Finalized(b) => {
                    log::info!(
                        "Successfully submitted call {}::{} {} at {}!",
                        pallet,
                        name,
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
