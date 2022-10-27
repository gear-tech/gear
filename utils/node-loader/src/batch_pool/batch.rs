use self::{
    create_program::CreateProgramBatchOutput, send_message::SendMessageBatchOutput,
    upload_code::UploadCodeBatchOutput, upload_program::UploadProgramBatchOutput,
};
use super::Seed;
use crate::utils;
use anyhow::{anyhow, Result};
pub use create_program::{CreateProgramArgs, CreateProgramArgsInner};
use futures::Future;
use gclient::{GearApi, Result as GClientResult};
use once_cell::sync::OnceCell;
use parking_lot::{Mutex, MutexGuard};
pub use send_message::{SendMessageArgs, SendMessageArgsInner};
use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    sync::atomic::{AtomicU32, Ordering},
};
pub use upload_code::UploadCodeArgs;
pub use upload_program::{UploadProgramArgs, UploadProgramArgsInner};

mod create_program;
mod send_message;
mod upload_code;
mod upload_program;

static AVAILABLE_NONCE: OnceCell<AtomicU32> = OnceCell::new();
static MISSED_NONCES: OnceCell<Mutex<MinHeap>> = OnceCell::new();

type MinHeap = BinaryHeap<Reverse<u32>>;
type MissedNoncesGuard<'a> = MutexGuard<'a, MinHeap>;

pub enum Batch {
    UploadProgram(Vec<UploadProgramArgs>),
    UploadCode(Vec<UploadCodeArgs>),
    SendMessage(Vec<SendMessageArgs>),
    CreateProgram(Vec<CreateProgramArgs>),
}

pub struct BatchWithSeed {
    pub seed: Seed,
    pub batch: Batch,
}

impl BatchWithSeed {
    pub fn batch_str(&self) -> &'static str {
        match &self.batch {
            Batch::UploadProgram(_) => "upload_program",
            Batch::UploadCode(_) => "upload_code",
            Batch::SendMessage(_) => "send_message",
            Batch::CreateProgram(_) => "create_program",
        }
    }
}

impl From<BatchWithSeed> for Batch {
    fn from(other: BatchWithSeed) -> Self {
        other.batch
    }
}

impl From<(Seed, Batch)> for BatchWithSeed {
    fn from((seed, batch): (Seed, Batch)) -> Self {
        Self { seed, batch }
    }
}

impl From<BatchWithSeed> for (Seed, Batch) {
    fn from(BatchWithSeed { seed, batch }: BatchWithSeed) -> Self {
        (seed, batch)
    }
}

// todo decide how to return errors and convert them
// todo possible restructure
#[derive(Clone)]
pub struct BatchSender {
    api: GearApi,
}

impl BatchSender {
    pub async fn try_new(endpoint: String, user: String) -> Result<Self> {
        let api = GearApi::init_with(utils::str_to_wsaddr(endpoint), user).await?;
        let available_nonce = api.rpc_nonce().await?;

        tracing::info!("Batch sender starts with nonce {available_nonce}");

        let an = AVAILABLE_NONCE.get_or_init(|| AtomicU32::new(available_nonce));
        let mn = MISSED_NONCES.get_or_init(|| Mutex::new(MinHeap::new()));
        if an.load(Ordering::Relaxed) != available_nonce || !mn.lock().is_empty() {
            return Err(anyhow!("Duplicate batch sender."));
        }

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
        catch_missed_nonce(&r, nonce);

        r.map_err(utils::try_node_dead_err)
    }

    fn prepare_api_for_call(&self) -> (GearApi, u32) {
        let nonce = self.call_nonce().expect("nonce storages are initialized");
        let mut api = self.api.clone();
        api.set_nonce(nonce);

        (api, nonce)
    }

    fn call_nonce(&self) -> Result<u32> {
        let ret_nonce;

        if is_empty_missed_nonce()? {
            ret_nonce = increment_nonce()?;
            tracing::info!("Call with a new nonce: {ret_nonce}");
        } else {
            ret_nonce = pop_missed_nonce()?;
            tracing::info!("Call with repeated nonce: {ret_nonce}");
        }

        Ok(ret_nonce)
    }
}

fn is_empty_missed_nonce() -> Result<bool> {
    hold_missed_nonces().map(|mn| mn.is_empty())
}

fn increment_nonce() -> Result<u32> {
    AVAILABLE_NONCE
        .get()
        .ok_or(anyhow!("Not initialized missed nonces storage"))
        .map(|an| an.fetch_add(1, Ordering::Relaxed))
}

fn pop_missed_nonce() -> Result<u32> {
    hold_missed_nonces()?
        .pop()
        .map(|Reverse(v)| v)
        .ok_or(anyhow!("empty missed nonce storage"))
}

fn hold_missed_nonces<'a>() -> Result<MissedNoncesGuard<'a>> {
    MISSED_NONCES
        .get()
        .map(|m| m.lock())
        .ok_or(anyhow!("Not initialized missed nonces storage"))
}

fn catch_missed_nonce<T>(batch_res: &GClientResult<T>, nonce: u32) {
    match batch_res {
        Err(err) => {
            if err
                .to_string()
                .contains(utils::EXHAUST_BLOCK_LIMIT_ERROR_STR)
            {
                hold_missed_nonces()
                    .expect("initialized")
                    .push(Reverse(nonce));
            }
        }
        _ => {}
    }
}

#[test]
fn test_min_heap_order() {
    use rand::Rng;

    let mut test_array = [0u32; 512];
    let mut thread_rng = rand::thread_rng();
    thread_rng.fill(&mut test_array);

    let mut min_heap = MinHeap::from_iter(test_array.into_iter().map(Reverse));

    test_array.sort_unstable();

    for expected in test_array {
        let actual = min_heap.pop().expect("same size as iterator");
        assert_eq!(
            Reverse(expected),
            actual,
            "failed test with test array {test_array:?}"
        );
    }
}
