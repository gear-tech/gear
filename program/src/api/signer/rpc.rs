//! RPC calls with signer
use crate::{
    api::{signer::Signer, types::GasInfo},
    result::Result,
};
use gear_core::ids::{CodeId, MessageId, ProgramId};
use subxt::ext::sp_core::H256;

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
        self.api
            .calculate_create_gas(
                self.source(),
                code_id,
                payload,
                value,
                allow_other_panics,
                at,
            )
            .await
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
        self.api
            .calculate_upload_gas(self.source(), code, payload, value, allow_other_panics, at)
            .await
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
        self.api
            .calculate_handle_gas(
                self.source(),
                destination,
                payload,
                value,
                allow_other_panics,
                at,
            )
            .await
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
        self.api
            .calculate_reply_gas(
                self.source(),
                message_id,
                exit_code,
                payload,
                value,
                allow_other_panics,
                at,
            )
            .await
    }
}
