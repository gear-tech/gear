use std::cell::RefCell;

use super::*;
use crate::tests::init_signer_with_keys;

thread_local! {
    static BATCH: RefCell<Option<MultisignedBatchCommitment>> = const { RefCell::new(None) };
}

pub fn with_batch(f: impl FnOnce(Option<&MultisignedBatchCommitment>)) {
    BATCH.with_borrow(|storage| f(storage.as_ref()));
}

struct DummyCommitter;

#[async_trait]
impl BatchCommitter for DummyCommitter {
    fn clone_boxed(&self) -> Box<dyn BatchCommitter> {
        Box::new(DummyCommitter)
    }

    async fn commit_batch(self: Box<Self>, batch: MultisignedBatchCommitment) -> Result<H256> {
        BATCH.with_borrow_mut(|storage| storage.replace(batch));
        Ok(H256::random())
    }
}

#[async_trait]
pub trait WaitForEvent {
    async fn wait_for_event(self) -> Result<(Box<dyn ValidatorSubService>, ControlEvent)>;
}

#[async_trait]
impl WaitForEvent for Box<dyn ValidatorSubService> {
    async fn wait_for_event(self) -> Result<(Box<dyn ValidatorSubService>, ControlEvent)> {
        wait_for_event_inner(self).await
    }
}

pub fn mock_validator_context() -> (ValidatorContext, Vec<PublicKey>) {
    let (signer, _, mut keys) = init_signer_with_keys(10);

    let ctx = ValidatorContext {
        slot_duration: Duration::from_secs(1),
        threshold: 1,
        router_address: 12345.into(),
        pub_key: keys.pop().unwrap(),
        signer,
        db: Database::memory(),
        committer: Box::new(DummyCommitter),
        pending_events: VecDeque::new(),
        output: VecDeque::new(),
    };

    (ctx, keys)
}

async fn wait_for_event_inner(
    s: Box<dyn ValidatorSubService>,
) -> Result<(Box<dyn ValidatorSubService>, ControlEvent)> {
    struct Dummy(Option<Box<dyn ValidatorSubService>>);

    impl Future for Dummy {
        type Output = Result<ControlEvent>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let mut s = self.0.take().unwrap().poll(cx)?;
            let res = s
                .context_mut()
                .output
                .pop_front()
                .map(|event| Poll::Ready(Ok(event)))
                .unwrap_or(Poll::Pending);
            self.0 = Some(s);
            res
        }
    }

    let mut dummy = Dummy(Some(s));
    let event = (&mut dummy).await?;
    Ok((dummy.0.unwrap(), event))
}
