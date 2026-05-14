// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{ProcessorExt, Result, service::SubService};
use ethexe_common::{
    CodeAndIdUnchecked,
    db::{CodesStorageRO, CodesStorageRW},
};
use ethexe_db::Database;
use ethexe_processor::{ProcessedCodeInfo, ValidCodeInfo};
use futures::{FutureExt, StreamExt, future::BoxFuture, stream::FuturesUnordered};
use gprimitives::CodeId;
use metrics::Gauge;
use std::{
    future,
    task::{Context, Poll},
};

/// Metrics for the [`CodesSubService`].
#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_compute_codes")]
struct Metrics {
    /// The number of currently processing codes.
    processing_codes: Gauge,
}

pub struct CodesSubService<P: ProcessorExt> {
    db: Database,
    processor: P,
    metrics: Metrics,

    processions: FuturesUnordered<BoxFuture<'static, Result<CodeId>>>,
}

impl<P: ProcessorExt> CodesSubService<P> {
    pub fn new(db: Database, processor: P) -> Self {
        Self {
            db,
            processor,
            metrics: Metrics::default(),
            processions: FuturesUnordered::new(),
        }
    }

    pub fn receive_code_to_process(&mut self, code_and_id: CodeAndIdUnchecked) {
        let code_id = code_and_id.code_id;
        if let Some(valid) = self.db.code_valid(code_id) {
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
            self.processions.push(future::ready(Ok(code_id)).boxed());
        } else {
            let db = self.db.clone();
            let mut processor = self.processor.clone();

            self.processions.push(
                async move {
                    let ProcessedCodeInfo { code_id, valid } =
                        processor.process_code(code_and_id).await?;
                    if let Some(ValidCodeInfo {
                        code,
                        instrumented_code,
                        code_metadata,
                    }) = valid
                    {
                        db.set_original_code(&code);
                        db.set_instrumented_code(
                            ethexe_runtime_common::VERSION,
                            code_id,
                            instrumented_code,
                        );
                        db.set_code_metadata(code_id, code_metadata);
                        db.set_code_valid(code_id, true);
                    } else {
                        db.set_code_valid(code_id, false);
                    }

                    Ok(code_id)
                }
                .boxed(),
            );
        }

        self.metrics
            .processing_codes
            .set(self.processions.len() as f64);
    }
}

impl<P: ProcessorExt> SubService for CodesSubService<P> {
    type Output = CodeId;

    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Output>> {
        if let Poll::Ready(Some(res)) = self.processions.poll_next_unpin(cx) {
            self.metrics
                .processing_codes
                .set(self.processions.len() as f64);
            return Poll::Ready(res);
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;
    use ethexe_common::{CodeAndId, mock::Tap};
    use gear_core::code::{InstantiatedSectionSizes, InstrumentedCode};

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn process_code() {
        let db = Database::memory();
        let code_and_id = CodeAndId::new(vec![1, 2, 3, 4]);
        let processor = MockProcessor::with_default_valid_code()
            .tap_mut(|p| p.process_codes_result.as_mut().unwrap().code_id = code_and_id.code_id());
        let mut service = CodesSubService::new(db.clone(), processor);

        service.receive_code_to_process(code_and_id.clone().into_unchecked());
        assert_eq!(service.next().await.unwrap(), code_and_id.code_id());
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn process_already_validated_code() {
        let db = Database::memory();
        let mut service =
            CodesSubService::new(db.clone(), MockProcessor::with_default_valid_code());

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
        assert_eq!(service.next().await.unwrap(), code_id);

        let code_and_id = CodeAndId::new(vec![100, 101, 102, 103]);
        let code_id = code_and_id.code_id();
        db.set_code_valid(code_id, false);
        service.receive_code_to_process(code_and_id.into_unchecked());
        assert_eq!(service.next().await.unwrap(), code_id);
    }

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn process_invalid_code() {
        let db = Database::memory();
        let mut service = CodesSubService::new(db.clone(), MockProcessor::default());

        let code_and_id = CodeAndId::new(vec![1, 2, 3, 4]);
        let code_id = code_and_id.code_id();
        service.receive_code_to_process(code_and_id.into_unchecked());
        assert_eq!(service.next().await.unwrap(), code_id);
        assert_eq!(db.code_valid(code_id), Some(false));
    }
}
