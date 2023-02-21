//! gear api calls
use crate::api::{
    config::GearConfig,
    signer::Signer,
    types::{InBlock, TxStatus},
};
use anyhow::anyhow;
use async_recursion::async_recursion;
use subxt::{
    ext::codec::Encode,
    tx::{StaticTxPayload, TxPayload, TxProgress},
    Error as SubxtError, OnlineClient,
};

type TxProgressT = TxProgress<GearConfig, OnlineClient<GearConfig>>;

const ERRORS_REQUIRE_RETRYING: [&str; 2] = ["Connection reset by peer", "Connection refused"];

mod balances {
    use crate::api::{generated::api::tx, signer::Signer, types::InBlock};
    use subxt::ext::sp_runtime::AccountId32;

    impl Signer {
        /// `pallet_balances::transfer`
        pub async fn transfer(&self, destination: impl Into<AccountId32>, value: u128) -> InBlock {
            let ex = tx().balances().transfer(destination.into().into(), value);
            self.process(ex).await
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

            self.process(ex).await
        }

        /// `pallet_gear::claim_value`
        pub async fn claim_value(&self, message_id: MessageId) -> InBlock {
            let ex = tx().gear().claim_value(message_id.into());

            self.process(ex).await
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

            self.process(ex).await
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

            self.process(ex).await
        }

        /// `pallet_gear::upload_code`
        pub async fn upload_code(&self, code: Vec<u8>) -> InBlock {
            let ex = tx().gear().upload_code(code);

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
        ) -> InBlock {
            let ex = tx()
                .gear()
                .upload_program(code, salt, payload, gas_limit, value);

            self.process(ex).await
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

    /// Wrapper for submit and watch with error handling.
    #[async_recursion(?Send)]
    async fn sign_and_submit_then_watch<CallData: Encode>(
        &self,
        tx: &StaticTxPayload<CallData>,
        counter: u16,
    ) -> Result<TxProgressT, SubxtError> {
        let process = if let Some(nonce) = self.nonce {
            self.api
                .tx()
                .create_signed_with_nonce(tx, &self.signer, nonce, Default::default())?
                .submit_and_watch()
                .await
        } else {
            self.api
                .tx()
                .sign_and_submit_then_watch_default(tx, &self.signer)
                .await
        };

        if counter >= self.retry {
            return process;
        }

        // TODO: Add more patterns for this retrying job.
        if let Err(SubxtError::Rpc(rpc_error)) = &process {
            let error_string = rpc_error.to_string();
            for error in ERRORS_REQUIRE_RETRYING {
                if error_string.contains(error) {
                    return self.sign_and_submit_then_watch(tx, counter + 1).await;
                }
            }
        }

        process
    }

    /// Listen transaction process and print logs.
    pub async fn process<CallData: Encode>(&self, tx: StaticTxPayload<CallData>) -> InBlock {
        use subxt::tx::TxStatus::*;

        let before = self.balance().await?;
        let mut process = self.sign_and_submit_then_watch(&tx, 0).await?;

        // Get extrinsic details.
        let (pallet, name) = {
            if let Some(details) = tx.validation_details() {
                (details.pallet_name, details.call_name)
            } else {
                ("Unknown", "Unknown")
            }
        };

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
