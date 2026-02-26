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

#[cfg(test)]
use crate::tests::MockProcessor;
use crate::{
    ComputeError, ComputeEvent, ProcessorExt, Result,
    codes::CodesSubService,
    compute::{ComputeConfig, ComputeSubService},
    prepare::PrepareSubService,
};
use ethexe_common::{Announce, CodeAndIdUnchecked, injected::Promise};
use ethexe_db::Database;
use ethexe_processor::{Processor, ProcessorConfig};
use futures::{Stream, stream::FusedStream};
use gprimitives::H256;
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::mpsc;

pub struct ComputeService<P: ProcessorExt = Processor> {
    codes_sub_service: CodesSubService<P>,
    prepare_sub_service: PrepareSubService,
    compute_sub_service: ComputeSubService<P>,
    promise_receiver: Option<mpsc::UnboundedReceiver<Promise>>,
}

impl<P: ProcessorExt> ComputeService<P> {
    // TODO #4550: consider to create Processor inside ComputeService

    pub fn process_code(&mut self, code_and_id: CodeAndIdUnchecked) {
        self.codes_sub_service.receive_code_to_process(code_and_id);
    }

    pub fn prepare_block(&mut self, block: H256) {
        self.prepare_sub_service.receive_block_to_prepare(block);
    }

    pub fn compute_announce(&mut self, announce: Announce, should_produce_promises: bool) {
        self.compute_sub_service
            .receive_announce_to_compute(announce, should_produce_promises);
    }
}

impl<P: ProcessorExt> Stream for ComputeService<P> {
    type Item = Result<ComputeEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(result) = self.codes_sub_service.poll_next(cx) {
            match result {
                Ok(code_id) => {
                    self.prepare_sub_service.receive_processed_code(code_id);
                    return Poll::Ready(Some(Ok(ComputeEvent::CodeProcessed(code_id))));
                }
                Err(e) => {
                    return Poll::Ready(Some(Err(e)));
                }
            }
        };

        if let Poll::Ready(result) = self.prepare_sub_service.poll_next(cx) {
            return Poll::Ready(Some(result.map(ComputeEvent::from)));
        };

        if let Poll::Ready(result) = self.compute_sub_service.poll_next(cx) {
            return Poll::Ready(Some(result.map(ComputeEvent::AnnounceComputed)));
        };

        if let Some(ref mut receiver) = self.promise_receiver
            && let Poll::Ready(maybe_promise) = receiver.poll_recv(cx)
        {
            return Poll::Ready(Some(
                maybe_promise
                    .map(Into::into)
                    .ok_or_else(|| ComputeError::PromiseSenderDropped),
            ));
        }

        Poll::Pending
    }
}

impl<P: ProcessorExt> FusedStream for ComputeService<P> {
    fn is_terminated(&self) -> bool {
        false
    }
}

pub(crate) trait SubService: Unpin + Send + 'static {
    type Output;

    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Result<Self::Output>>;

    #[cfg(test)]
    async fn next(&mut self) -> Result<Self::Output> {
        futures::future::poll_fn(|cx| self.poll_next(cx)).await
    }
}

/// Module provides a builder for [`ComputeService`].
/// The [`builder::Builder`] must be used for both production and testing purposes.
pub(crate) mod builder {
    use super::*;

    // Builder environments
    #[cfg(test)]
    #[derive(Default)]
    pub struct Mock;
    #[derive(Default)]
    pub struct Production;

    // Builder states
    #[derive(Default)]
    pub struct Set;
    #[derive(Default)]
    pub struct Unset;

    /// A type-state builder for [`ComputeService`].
    /// Provides the easy construction the [`ComputeService`] for both
    /// testing and production environments.
    #[derive(Default)]
    pub struct Builder<Env, Config = Unset, DB = Unset, Processor = Unset> {
        config: Option<ComputeConfig>,
        db: Option<Database>,
        processor_config: Option<ProcessorConfig>,

        _state: PhantomData<(Env, Config, DB, Processor)>,
    }

    /// Mock builder uses defaults when fields are None; type-states are fixed to Set.
    #[cfg(test)]
    impl Builder<Mock, Set, Set, Set> {
        /// Creates a new mock builder.
        pub(crate) fn mock() -> Self {
            Self::default()
        }

        #[allow(unused)]
        pub(crate) fn with_config(mut self, config: ComputeConfig) -> Self {
            self.config = Some(config);
            self
        }

        #[allow(unused)]
        pub(crate) fn with_db(mut self, db: Database) -> Self {
            self.db = Some(db);
            self
        }

        /// Creates a [`ComputeService<MockProcessor>`] from a mock builder.
        pub(crate) fn build(self) -> ComputeService<MockProcessor> {
            let processor = MockProcessor;
            let config = self.config.unwrap_or(ComputeConfig::without_quarantine());
            let db = self.db.unwrap_or(Database::memory());

            ComputeService {
                prepare_sub_service: PrepareSubService::new(db.clone()),
                compute_sub_service: ComputeSubService::new(config, db.clone(), processor.clone()),
                codes_sub_service: CodesSubService::new(db, processor),
                promise_receiver: None,
            }
        }
    }

    impl Builder<Production, Unset, Unset, Unset> {
        /// Creates a new production builder.
        pub fn production() -> Self {
            Self::default()
        }

        /// Creates a new production builder with default configs: [`ComputeConfig`], [`ProcessorConfig`].
        #[cfg(test)]
        pub fn production_with_defaults(db: Database) -> Builder<Production, Set, Set, Set> {
            Self::production()
                .db(db)
                .compute_config(ComputeConfig::without_quarantine())
                .processor_config(ProcessorConfig::default())
        }
    }

    // Important: production builder allows to set variable only once.

    impl<D, P> Builder<Production, Unset, D, P> {
        pub fn compute_config(self, config: ComputeConfig) -> Builder<Production, Set, D, P> {
            Builder {
                config: Some(config),
                db: self.db,
                processor_config: self.processor_config,
                _state: PhantomData,
            }
        }
    }

    impl<C, P> Builder<Production, C, Unset, P> {
        pub fn db(self, db: Database) -> Builder<Production, C, Set, P> {
            Builder {
                config: self.config,
                db: Some(db),
                processor_config: self.processor_config,
                _state: PhantomData,
            }
        }
    }

    impl<C, D> Builder<Production, C, D, Unset> {
        pub fn processor_config(self, config: ProcessorConfig) -> Builder<Production, C, D, Set> {
            Builder {
                config: self.config,
                db: self.db,
                processor_config: Some(config),
                _state: PhantomData,
            }
        }
    }

    /// Implementation for builder with all filled fields.
    impl Builder<Production, Set, Set, Set> {
        /// Creates the [`ComputeService`] from a production builder.
        pub fn build(self) -> Result<ComputeService> {
            let (config, db, processor_config) = self.into_parts_unchecked();

            let (promise_out_tx, promise_receiver) = mpsc::unbounded_channel();
            let processor =
                Processor::with_config(processor_config, db.clone(), Some(promise_out_tx))?;

            Ok(ComputeService {
                prepare_sub_service: PrepareSubService::new(db.clone()),
                compute_sub_service: ComputeSubService::new(config, db.clone(), processor.clone()),
                codes_sub_service: CodesSubService::new(db, processor),
                promise_receiver: Some(promise_receiver),
            })
        }

        /// Reconstructs builder into parts..
        fn into_parts_unchecked(self) -> (ComputeConfig, Database, ProcessorConfig) {
            (
                self.config.unwrap(),
                self.db.unwrap(),
                self.processor_config.unwrap(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ComputeServiceBuilder;

    use super::*;
    use ethexe_common::{CodeAndIdUnchecked, db::*, mock::*};
    use ethexe_db::Database as DB;
    use futures::StreamExt;
    use gear_core::ids::prelude::CodeIdExt;
    use gprimitives::CodeId;

    /// Test ComputeService block preparation functionality
    #[tokio::test]
    async fn prepare_block() {
        gear_utils::init_default_logger();

        let db = DB::memory();
        let mut service = ComputeServiceBuilder::mock().with_db(db.clone()).build();

        let chain = BlockChain::mock(1).setup(&db);
        let block = chain.blocks[1].to_simple().next_block().setup(&db);

        // Request block preparation
        service.prepare_block(block.hash);

        // Poll service to process the preparation request
        let event = service.next().await.unwrap().unwrap();
        assert_eq!(event, ComputeEvent::BlockPrepared(block.hash));

        // Verify block is marked as prepared in DB
        assert!(db.block_meta(block.hash).prepared);
    }

    /// Test ComputeService block processing functionality
    #[tokio::test]
    async fn compute_announce() {
        gear_utils::init_default_logger();

        let db = DB::memory();
        let mut service = ComputeServiceBuilder::mock().with_db(db.clone()).build();

        let chain = BlockChain::mock(1).setup(&db);

        let block = chain.blocks[1].to_simple().next_block().setup(&db);

        service.prepare_block(block.hash);
        let event = service.next().await.unwrap().unwrap();
        assert_eq!(event, ComputeEvent::BlockPrepared(block.hash));

        // Request computation
        let announce = Announce {
            block_hash: block.hash,
            parent: chain.block_top_announce_hash(1),
            gas_allowance: Some(42),
            injected_transactions: vec![],
        };
        let announce_hash = announce.to_hash();
        service.compute_announce(announce, false);

        // Poll service to process the block
        let event = service.next().await.unwrap().unwrap();
        assert_eq!(event, ComputeEvent::AnnounceComputed(announce_hash));

        // Verify block is marked as computed in DB
        assert!(db.announce_meta(announce_hash).computed);
    }

    /// Test ComputeService code processing functionality
    #[tokio::test]
    async fn process_code() {
        gear_utils::init_default_logger();

        let db = DB::memory();
        let mut service = ComputeServiceBuilder::mock().with_db(db.clone()).build();

        // Create test code
        let code = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]; // Simple WASM header
        let code_id = CodeId::generate(&code);

        let code_and_id = CodeAndIdUnchecked { code, code_id };

        // Verify code is not yet in DB
        assert!(db.code_valid(code_id).is_none());

        // Request code processing
        service.process_code(code_and_id);

        // Poll service to process the code
        let event = service.next().await.unwrap().unwrap();

        // Should receive CodeProcessed event with correct code_id
        match event {
            ComputeEvent::CodeProcessed(processed_code_id) => {
                assert_eq!(processed_code_id, code_id);
            }
            _ => panic!("Expected CodeProcessed event"),
        }
    }
}
