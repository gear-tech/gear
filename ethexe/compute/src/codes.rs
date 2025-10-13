use crate::{ComputeError, ProcessorExt, Result};
use ethexe_common::{CodeAndIdUnchecked, db::CodesStorageRead};
use ethexe_db::Database;
use futures::Stream;
use gprimitives::CodeId;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::task::JoinSet;

pub struct CodesSubService<P: ProcessorExt> {
    db: Database,
    processor: P,

    processions: JoinSet<Result<CodeId>>,
}

impl<P: ProcessorExt> CodesSubService<P> {
    pub fn new(db: Database, processor: P) -> Self {
        Self {
            db,
            processor,
            processions: JoinSet::new(),
        }
    }

    pub fn receive_code_to_process(&mut self, code_and_id: CodeAndIdUnchecked) {
        let code_id = code_and_id.code_id;
        if let Some(valid) = self.db.code_valid(code_id) {
            // TODO: #4712 test this case
            log::warn!("Code {code_id:?} already processed");

            if valid {
                debug_assert!(
                    self.db.original_code_exists(code_id),
                    "Code {code_id:?} must exist in database"
                );
                debug_assert!(
                    self.db
                        .instrumented_code_exists(ethexe_runtime_common::VERSION, code_id),
                    "Instrumented code {code_id:?} must exist in database"
                );
            }

            self.processions.spawn(async move { Ok(code_id) });
        } else {
            let mut processor = self.processor.clone();

            self.processions.spawn_blocking(move || {
                processor
                    .process_upload_code(code_and_id)
                    .map(|_valid| code_id)
            });
        }
    }
}

impl<P: ProcessorExt> Stream for CodesSubService<P> {
    type Item = Result<CodeId>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        futures::ready!(self.processions.poll_join_next(cx))
            .map(|res| res.map_err(ComputeError::CodeProcessJoin)?)
            .map_or(Poll::Pending, |res| Poll::Ready(Some(res)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;
    use ethexe_common::{CodeAndId, db::*};
    use futures::StreamExt;
    use gear_core::code::{InstantiatedSectionSizes, InstrumentedCode};

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn process_code() {
        let db = Database::memory();
        let mut service = CodesSubService::new(db.clone(), MockProcessor);

        let code_and_id = CodeAndId::new(vec![1, 2, 3, 4]);

        service.receive_code_to_process(code_and_id.clone().into_unchecked());
        assert_eq!(
            service.next().await.unwrap().unwrap(),
            code_and_id.code_id()
        );
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn process_already_validated_code() {
        let db = Database::memory();
        let mut service = CodesSubService::new(db.clone(), MockProcessor);

        let code_and_id = CodeAndId::new(vec![1, 2, 3, 4]);
        let code_id = code_and_id.code_id();
        db.set_code_valid(code_id, true);
        db.set_original_code(code_and_id.code());
        db.set_instrumented_code(
            ethexe_runtime_common::VERSION,
            code_id,
            InstrumentedCode::new(
                vec![5, 6, 7, 8],
                InstantiatedSectionSizes::new(1, 1, 1, 1, 1, 1),
            ),
        );
        service.receive_code_to_process(code_and_id.into_unchecked());
        assert_eq!(service.next().await.unwrap().unwrap(), code_id);

        let code_and_id = CodeAndId::new(vec![100, 101, 102, 103]);
        let code_id = code_and_id.code_id();
        db.set_code_valid(code_id, false);
        service.receive_code_to_process(code_and_id.into_unchecked());
        assert_eq!(service.next().await.unwrap().unwrap(), code_id);
    }
}
