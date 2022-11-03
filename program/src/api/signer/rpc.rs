//! gear api rpc methods
use crate::{
    api::{signer::Signer, types::GasInfo},
    result::Result,
};
use gear_core::ids::{CodeId, MessageId, ProgramId};
use subxt::{ext::sp_core::H256, rpc::rpc_params};

impl Signer {
    /// public key of the signer in H256
    pub fn source(&self) -> H256 {
        AsRef::<[u8; 32]>::as_ref(self.signer.account_id()).into()
    }

    /// gear_calculateInitCreateGas
    pub async fn calculate_create_gas(
        &self,
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
                    self.source(),
                    H256(code_id.into()),
                    hex::encode(payload),
                    u64::try_from(value).unwrap_or(u64::MAX),
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
                    self.source(),
                    hex::encode(code),
                    hex::encode(payload),
                    u64::try_from(value).unwrap_or(u64::MAX),
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
                    self.source(),
                    H256(destination.into()),
                    hex::encode(payload),
                    u64::try_from(value).unwrap_or(u64::MAX),
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
                    self.source(),
                    H256(message_id.into()),
                    exit_code,
                    hex::encode(payload),
                    u64::try_from(value).unwrap_or(u64::MAX),
                    allow_other_panics,
                    at
                ],
            )
            .await
            .map_err(Into::into)
    }
}
