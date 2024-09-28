use crate::Processor;
use anyhow::Result;
use ethexe_db::CodesStorage;
use ethexe_runtime_common::state::{Dispatch, HashAndLen, MaybeHash, Storage};
use gear_core::message::Payload;
use gprimitives::{CodeId, H256};
use std::collections::VecDeque;

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

        let mut state = self
            .db
            .read_state(state_hash)
            .ok_or_else(|| anyhow::anyhow!("program should exist"))?;

        anyhow::ensure!(state.program.is_active(), "program should be active");

        let queue = if let MaybeHash::Hash(HashAndLen {
            hash: queue_hash, ..
        }) = state.queue_hash
        {
            let mut queue = self
                .db
                .read_queue(queue_hash)
                .ok_or_else(|| anyhow::anyhow!("queue should exist if hash present"))?;

            queue.extend(dispatches);

            queue
        } else {
            VecDeque::from(dispatches)
        };

        state.queue_hash = self.db.write_queue(queue).into();

        Ok(self.db.write_state(state))
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
}
