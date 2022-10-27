use super::batch::{
    CreateProgramArgs, CreateProgramBatchOutput, SendMessageArgs, SendMessageBatchOutput,
    UploadCodeArgs, UploadCodeBatchOutput, UploadProgramArgs, UploadProgramBatchOutput,
};
use crate::utils;
use anyhow::Result;
use futures::Future;
use gclient::{GearApi, Result as GClientResult};

mod nonce;

#[derive(Clone)]
pub struct BatchSender {
    api: GearApi,
}

impl BatchSender {
    pub async fn try_new(endpoint: String, user: String) -> Result<Self> {
        let api = GearApi::init_with(utils::str_to_wsaddr(endpoint), user).await?;
        let available_nonce = api.rpc_nonce().await?;

        tracing::info!("Batch sender starts with nonce {available_nonce}");

        nonce::init_nonces(available_nonce)?;

        Ok(Self { api })
    }

    pub fn into_gear_api(self) -> GearApi {
        self.api
    }

    pub async fn upload_program_batch(
        &mut self,
        args: Vec<UploadProgramArgs>,
    ) -> Result<UploadProgramBatchOutput> {
        self.call(|api| async move {
            api.upload_program_bytes_batch(utils::convert_iter(args))
                .await
        })
        .await
    }

    pub async fn upload_code_batch(
        &mut self,
        args: Vec<UploadCodeArgs>,
    ) -> Result<UploadCodeBatchOutput> {
        self.call(|api| async move {
            api.upload_code_batch(utils::convert_iter::<Vec<_>, _>(args))
                .await
        })
        .await
    }

    pub async fn send_message_batch(
        &mut self,
        args: Vec<SendMessageArgs>,
    ) -> Result<SendMessageBatchOutput> {
        self.call(|api| async move {
            api.send_message_bytes_batch(utils::convert_iter(args))
                .await
        })
        .await
    }

    pub async fn create_program_batch(
        &mut self,
        args: Vec<CreateProgramArgs>,
    ) -> Result<CreateProgramBatchOutput> {
        self.call(|api| async move {
            api.create_program_bytes_batch(utils::convert_iter(args))
                .await
        })
        .await
    }

    async fn call<T, F: Future<Output = GClientResult<T>>>(
        &mut self,
        f: impl FnOnce(GearApi) -> F,
    ) -> Result<T> {
        let (api, nonce) = self.prepare_api_for_call();

        let r = utils::with_timeout(f(api)).await?;
        nonce::catch_missed_nonce(&r, nonce).expect("missed nonces storage is initialized");

        r.map_err(Into::into)
    }

    fn prepare_api_for_call(&self) -> (GearApi, u32) {
        let nonce = self.call_nonce().expect("nonce storages are initialized");
        let mut api = self.api.clone();
        api.set_nonce(nonce);

        (api, nonce)
    }

    fn call_nonce(&self) -> Result<u32> {
        let ret_nonce;

        if nonce::is_empty_missed_nonce()? {
            ret_nonce = nonce::increment_nonce()?;
            tracing::info!("Call with a new nonce: {ret_nonce}");
        } else {
            ret_nonce = nonce::pop_missed_nonce()?;
            tracing::info!("Call with repeated nonce: {ret_nonce}");
        }

        Ok(ret_nonce)
    }
}
