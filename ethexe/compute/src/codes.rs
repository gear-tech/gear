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

use crate::{ComputeError, ProcessorExt, Result, service::SubService};
use ethexe_common::{CodeAndIdUnchecked, db::CodesStorageRO};
use ethexe_db::Database;
use gprimitives::CodeId;
use metrics::Gauge;
use std::task::{Context, Poll};
use tokio::task::JoinSet;

/// Metrics for the [`CodesSubService`].
#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_compute:codes")]
struct Metrics {
    /// The number of currently processing codes.
    processing_codes: Gauge,
}

pub struct CodesSubService<P: ProcessorExt> {
    db: Database,
    processor: P,
    metrics: Metrics,

    processions: JoinSet<Result<CodeId>>,
}

impl<P: ProcessorExt> CodesSubService<P> {
    pub fn new(db: Database, processor: P) -> Self {
        Self {
            db,
            processor,
            metrics: Metrics::default(),
            processions: JoinSet::new(),
        }
    }

    pub fn receive_code_to_process(&mut self, code_and_id: CodeAndIdUnchecked) {
        self.metrics.processing_codes.increment(1);

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

impl<P: ProcessorExt> SubService for CodesSubService<P> {
    type Output = CodeId;

    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Output>> {
        futures::ready!(self.processions.poll_join_next(cx))
            .map(|res| {
                // Decrement the processing codes metric.
                self.metrics.processing_codes.decrement(1);
                res.map_err(ComputeError::CodeProcessJoin)?
            })
            .map_or(Poll::Pending, Poll::Ready)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;
    use ethexe_common::{CodeAndId, db::*};
    use gear_core::code::{InstantiatedSectionSizes, InstrumentedCode};

    #[tokio::test]
    #[ntest::timeout(3000)]
    async fn process_code() {
        let db = Database::memory();
        let mut service = CodesSubService::new(db.clone(), MockProcessor);

        let code_and_id = CodeAndId::new(vec![1, 2, 3, 4]);

        service.receive_code_to_process(code_and_id.clone().into_unchecked());
        assert_eq!(service.next().await.unwrap(), code_and_id.code_id());
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
        assert_eq!(service.next().await.unwrap(), code_id);

        let code_and_id = CodeAndId::new(vec![100, 101, 102, 103]);
        let code_id = code_and_id.code_id();
        db.set_code_valid(code_id, false);
        service.receive_code_to_process(code_and_id.into_unchecked());
        assert_eq!(service.next().await.unwrap(), code_id);
    }
}
