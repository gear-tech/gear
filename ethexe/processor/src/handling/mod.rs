use crate::Processor;
use anyhow::Result;
use ethexe_db::CodesStorage;
use ethexe_runtime_common::state::{
    Dispatch, Mailbox, MaybeHash, MessageQueue, ProgramState, Storage,
};
use gear_core::message::Payload;
use gprimitives::{CodeId, H256};

pub(crate) mod events;
pub(crate) mod run;

impl Processor {
    pub(crate) fn handle_message_queueing(
        &mut self,
        state_hash: H256,
        dispatch: Dispatch,
    ) -> Result<H256> {
        self.handle_messages_queueing(state_hash, vec![dispatch])
    }

    pub(crate) fn handle_messages_queueing(
        &mut self,
        state_hash: H256,
        dispatches: Vec<Dispatch>,
    ) -> Result<H256> {
        if dispatches.is_empty() {
            return Ok(state_hash);
        }

        self.mutate_state(state_hash, |processor, state| {
            anyhow::ensure!(state.program.is_active(), "program should be active");

            state.queue_hash = processor
                .modify_queue(state.queue_hash.clone(), |queue| queue.extend(dispatches))?;

            Ok(())
        })
    }

    pub(crate) fn handle_payload(&mut self, payload: Vec<u8>) -> Result<MaybeHash> {
        let payload = Payload::try_from(payload)
            .map_err(|_| anyhow::anyhow!("payload should be checked on eth side"))?;

        let hash = payload
            .inner()
            .is_empty()
            .then_some(MaybeHash::Empty)
            .unwrap_or_else(|| self.db.write_payload(payload).into());

        Ok(hash)
    }

    /// Returns some CodeId in case of settlement and new code accepting.
    pub(crate) fn handle_new_code(
        &mut self,
        original_code: impl AsRef<[u8]>,
    ) -> Result<Option<CodeId>> {
        let mut executor = self.creator.instantiate()?;

        let original_code = original_code.as_ref();

        let Some(instrumented_code) = executor.instrument(original_code)? else {
            return Ok(None);
        };

        let code_id = self.db.set_original_code(original_code);

        self.db.set_instrumented_code(
            instrumented_code.instruction_weights_version(),
            code_id,
            instrumented_code,
        );

        Ok(Some(code_id))
    }

    pub(crate) fn modify_queue(
        &mut self,
        maybe_queue_hash: MaybeHash,
        f: impl FnOnce(&mut MessageQueue),
    ) -> Result<MaybeHash> {
        let mut queue = maybe_queue_hash.with_hash_or_default_result(|queue_hash| {
            self.db
                .read_queue(queue_hash)
                .ok_or_else(|| anyhow::anyhow!("failed to read queue by its hash"))
        })?;

        f(&mut queue);

        Ok(queue
            .is_empty()
            .then_some(MaybeHash::Empty)
            .unwrap_or_else(|| self.db.write_queue(queue).into()))
    }

    /// Usage: for optimized performance, please remove map entries if empty.
    pub(crate) fn modify_mailbox(
        &mut self,
        maybe_mailbox_hash: MaybeHash,
        f: impl FnOnce(&mut Mailbox),
    ) -> Result<MaybeHash> {
        let mut mailbox = maybe_mailbox_hash.with_hash_or_default_result(|mailbox_hash| {
            self.db
                .read_mailbox(mailbox_hash)
                .ok_or_else(|| anyhow::anyhow!("failed to read mailbox by its hash"))
        })?;

        f(&mut mailbox);

        Ok(mailbox
            .values()
            .all(|v| v.is_empty())
            .then_some(MaybeHash::Empty)
            .unwrap_or_else(|| self.db.write_mailbox(mailbox).into()))
    }

    pub(crate) fn mutate_state(
        &mut self,
        state_hash: H256,
        f: impl FnOnce(&mut Processor, &mut ProgramState) -> Result<()>,
    ) -> Result<H256> {
        self.mutate_state_returning(state_hash, f)
            .map(|((), hash)| hash)
    }

    pub(crate) fn mutate_state_returning<T>(
        &mut self,
        state_hash: H256,
        f: impl FnOnce(&mut Processor, &mut ProgramState) -> Result<T>,
    ) -> Result<(T, H256)> {
        let mut state = self.db.read_state(state_hash).ok_or_else(|| {
            anyhow::anyhow!("failed to find program state by hash ({state_hash})")
        })?;

        let res = f(self, &mut state)?;

        Ok((res, self.db.write_state(state)))
    }
}
