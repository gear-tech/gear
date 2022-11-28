//! gear api rpc methods
#![allow(clippy::too_many_arguments)]
use crate::{
    api::{types::GasInfo, Api},
    result::Result,
};
use gear_core::ids::{CodeId, MessageId, ProgramId};
use subxt::{ext::sp_core::H256, rpc::rpc_params};

impl Api {
    /// gear_calculateInitCreateGas
    pub async fn calculate_create_gas(
        &self,
        origin: H256,
        code_id: CodeId,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        at: Option<H256>,
    ) -> Result<GasInfo> {
        self.rpc()
            .request(
                "gear_calculateInitCreateGas",
                rpc_params![
                    origin,
                    H256(code_id.into()),
                    hex::encode(payload),
                    value,
                    allow_other_panics,
                    at
                ],
            )
            .await
            .map_err(Into::into)
    }

    /// gear_calculateInitUploadGas
    pub async fn calculate_upload_gas(
        &self,
        origin: H256,
        code: Vec<u8>,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        at: Option<H256>,
    ) -> Result<GasInfo> {
        self.rpc()
            .request(
                "gear_calculateInitUploadGas",
                rpc_params![
                    origin,
                    hex::encode(code),
                    hex::encode(payload),
                    value,
                    allow_other_panics,
                    at
                ],
            )
            .await
            .map_err(Into::into)
    }

    /// gear_calculateHandleGas
    pub async fn calculate_handle_gas(
        &self,
        origin: H256,
        destination: ProgramId,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        at: Option<H256>,
    ) -> Result<GasInfo> {
        self.rpc()
            .request(
                "gear_calculateHandleGas",
                rpc_params![
                    origin,
                    H256(destination.into()),
                    hex::encode(payload),
                    value,
                    allow_other_panics,
                    at
                ],
            )
            .await
            .map_err(Into::into)
    }

    /// gear_calculateReplyGas
    pub async fn calculate_reply_gas(
        &self,
        origin: H256,
        message_id: MessageId,
        exit_code: i32,
        payload: Vec<u8>,
        value: u128,
        allow_other_panics: bool,
        at: Option<H256>,
    ) -> Result<GasInfo> {
        self.rpc()
            .request(
                "gear_calculateReplyGas",
                rpc_params![
                    origin,
                    H256(message_id.into()),
                    exit_code,
                    hex::encode(payload),
                    value,
                    allow_other_panics,
                    at
                ],
            )
            .await
            .map_err(Into::into)
    }
}
