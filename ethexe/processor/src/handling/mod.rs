use crate::Processor;
use anyhow::Result;
use ethexe_db::CodesStorage;
use ethexe_runtime_common::state::{ComplexStorage as _, Dispatch};
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

        self.db.mutate_state(state_hash, |processor, state| {
            anyhow::ensure!(state.program.is_active(), "program should be active");

            state.queue_hash = processor
                .modify_queue(state.queue_hash.clone(), |queue| queue.extend(dispatches))?;

            Ok(())
        })
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
