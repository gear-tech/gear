// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! [`ethexe_malachite_core::Externalities`] glue for ethexe.
//!
//! ethexe-malachite-core is application-agnostic — it owns the BFT engine, the
//! libp2p swarm, and the persistent consensus state. Everything
//! ethexe-specific (block contents, validation rules, DB schema)
//! lives behind this trait.
//!
//! ## Map of responsibilities
//! - [`EthexeExternalities::process_mb_proposal`] — once
//!   ethexe-malachite-core has assembled and validated a proposal
//!   (parent already processed), persist the MB to the ethexe
//!   `mb_*` keyspace, propagate `last_advanced_eb`, and fire
//!   [`MalachiteEvent::BlockProposal`]. Called for sibling
//!   proposals too (one per round that produced an assembled MB) —
//!   only the finalized one ever flows through `process_mb_finalized`.
//! - [`EthexeExternalities::process_mb_finalized`] — flush the
//!   committed injected txs out of the mempool, advance
//!   `globals.latest_finalized_mb_hash`, and fire
//!   [`MalachiteEvent::BlockFinalized`].
//! - [`EthexeExternalities::build_block_above`] — when this node is
//!   proposer, wait for proposable content (a new EB past quarantine
//!   or a non-empty mempool), then assemble an [`Operations`] list.
//! - [`EthexeExternalities::validate_block_above`] — for an incoming
//!   peer proposal, run ethexe's quarantine + parent-link checks
//!   before voting.
//!
//! ## Storage layout
//!
//! All MB-keyed storage in the ethexe DB is keyed by the
//! `ethexe_malachite_core::Block` envelope hash (Blake2b over
//! `(parent_hash, height, payload_hash, reserved)`).
//! [`EthexeExternalities::process_mb_proposal`] writes a [`CompactMb`]
//! under that key (carrying parent + height + the Blake2b hash of
//! the [`Operations`] payload) and CAS-stores the `Operations`
//! blob; [`EthexeExternalities::process_mb_finalized`] reads both
//! back via the same key the consensus layer hands in.

use crate::{
    Mempool,
    decryption_shares::{DecryptionSharesStore, InsertOutcome},
    quarantine,
    tx_validity::{TxValidity, TxValidityChecker, eb_touched_programs},
    types::{ChainHead, CommitCertificate, MalachiteEvent},
};
use anyhow::{Context, Result, anyhow, bail, ensure};
use async_trait::async_trait;
use ethexe_common::{
    Acceptance, HashOf, MAX_TOUCHED_PROGRAMS_PER_MB, VerifiedData,
    db::{
        CompactMb, GlobalsStorageRO, GlobalsStorageRW, MbStorageRO, MbStorageRW, OnChainStorageRO,
    },
    injected::{
        InjectedTransaction, MAX_INJECTED_TRANSACTIONS_SIZE_PER_MB, PurgedTransaction,
        ShieldedTransaction, Transaction, TransactionHash, TransactionPurgedReason,
    },
    malachite::{MalachiteTdecContext, Operation, Operations, ShieldedTxDecryptionShare},
};
use ethexe_db::Database;
use ethexe_malachite_core::{
    Block, BlockPayload, EthexeVoteExtension, Externalities, MAX_BLOCK_PAYLOAD_BYTES,
};
use gear_tdec::bls12_381::{
    DecryptionShareSimple, SharedSecret, prepare_combine_simple, share_combine_simple,
};
use gprimitives::H256;
use gsigner::{Address, PublicDecryptionContext, tdec::TdecKeyStore};
use parity_scale_codec::{DecodeAll, Encode};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    sync::Arc,
};
use tokio::sync::{RwLock, mpsc::UnboundedSender};
use tracing::{debug, error, trace, warn};

/// Type alias for decryption keys provided in [Operation].
pub(crate) type DecryptionKeys = BTreeMap<HashOf<ShieldedTransaction>, SharedSecret>;

/// Constant parameters for [`EthexeExternalities`];
/// see [`crate::MalachiteServiceConfig`] for field semantics.
pub struct ExternalitiesConfig {
    /// Gas allowance per block.
    pub gas_allowance: u64,
    /// Quarantine depth an EB must clear before it can be advanced to.
    pub canonical_quarantine: u8,
    /// Extra producer-side anchor depth on top of `canonical_quarantine`.
    pub post_quarantine_delay: u32,
}

pub(crate) struct EthexeExternalities {
    /// Shared DB reference for all storage operations
    pub db: Database,
    /// Constant externalities config parameters
    pub cfg: ExternalitiesConfig,
    /// Optional mempool reference for injected-tx processing; `None` when not a validator.
    pub mempool: Option<Arc<dyn Mempool>>,
    /// Threshold-decryption context for the local validator.
    pub(crate) tdec_ctx: Option<MalachiteTdecContext>,
    /// Local threshold-decryption key store.
    pub(crate) tdec_store: TdecKeyStore,
    /// Verified decryption shares grouped by MB and transaction.
    pub(crate) decryption_shares: Arc<DecryptionSharesStore>,
    /// Reference to the latest chain head data.
    pub chain_head: Arc<ChainHead>,
    /// Pending service events queue.
    /// Release events from here only when their prerequisite EB is prepared.
    pub pending_events: RwLock<VecDeque<PendingEvent>>,
    /// Channel to poll events in MalachiteService.
    pub event_tx: UnboundedSender<Result<MalachiteEvent>>,
}

/// One outbound [`MalachiteEvent`] that can't be released until its
/// `prerequisite` Eth block is prepared in local DB.
pub(crate) struct PendingEvent {
    /// Event body
    pub event: MalachiteEvent,
    /// Prerequisite Eth block hash
    /// that must be prepared before this event can be emitted
    pub prerequisite: H256,
}

#[derive(Clone, Default)]
struct UnshieldingOutput {
    pub unshielded: Vec<(
        HashOf<ShieldedTransaction>,
        VerifiedData<InjectedTransaction>,
    )>,
    pub not_unshielded: Vec<PurgedTransaction>,
}

#[async_trait]
impl Externalities for EthexeExternalities {
    async fn extend_vote(&self, mb_hash: H256) -> Result<Option<EthexeVoteExtension>> {
        let Some(context) = self.tdec_ctx.as_ref() else {
            return Ok(None);
        };
        let compact = self
            .db
            .mb_compact_block(mb_hash)
            .with_context(|| format!("vote extension refers to unknown MB {mb_hash}"))?;
        let operations = self
            .db
            .operations(compact.operations_hash)
            .with_context(|| format!("operations for MB {mb_hash} are missing"))?;
        let transactions = operations
            .iter()
            .filter_map(|op| op.as_shielded().map(|signed| signed.data()))
            .collect::<Vec<_>>();
        if transactions.is_empty() {
            return Ok(None);
        }

        let my_address = context
            .contexts
            .iter()
            .find_map(|(address, participant)| {
                (participant.validator_public_key == context.my_context.validator_public_key)
                    .then_some(*address)
            })
            .context("local TDEC context is absent from validator contexts")?;
        let shares =
            self.provide_decryption_shares(mb_hash, &context.my_context, my_address, &transactions);

        Ok(Some(EthexeVoteExtension {
            sender: my_address,
            shares,
        }))
    }

    async fn verify_vote_extension(
        &self,
        mb_hash: H256,
        extension: &EthexeVoteExtension,
    ) -> Result<Acceptance<(), String>> {
        Ok(self.receive_decryption_shares(mb_hash, extension.sender, &extension.shares))
    }

    async fn process_mb_proposal(&self, mb_hash: H256, mb: Block) -> Result<()> {
        let operations = Operations::decode_all(&mut mb.payload.as_ref())
            .map_err(|e| anyhow!("decoding Operations from block payload bytes: {e}"))?;

        let parent = mb.parent_hash;

        let parent_advanced = parent
            .is_zero()
            .then(H256::zero)
            .unwrap_or_else(|| self.db.mb_meta(parent).last_advanced_eb);
        let last_advanced = operations
            .iter()
            .rev()
            .find_map(|tx| match tx {
                Operation::AdvanceTillEthereumBlock { block_hash } => Some(*block_hash),
                _ => None,
            })
            .unwrap_or(parent_advanced);

        let operations_hash = self.db.set_operations(operations.clone());
        self.db.set_mb_compact_block(
            mb_hash,
            CompactMb {
                parent,
                height: mb.height,
                operations_hash,
            },
        );
        self.db.mutate_mb_meta(mb_hash, |meta| {
            meta.last_advanced_eb = last_advanced;
        });

        let shielded_transactions = operations
            .iter()
            .filter_map(|op| op.as_shielded().map(|signed| signed.data()))
            .collect::<Vec<_>>();
        self.decryption_shares
            .register_block(mb_hash, shielded_transactions.iter().map(|tx| tx.to_hash()));

        // If decryption keys provided - decrypt shielded transactions and save them to database.
        if let Some(decryption_keys) = operations.iter().find_map(|op| match op {
            Operation::DecryptionKeys(keys) => Some(keys.clone()),
            _ => None,
        }) {
            self.process_unshielding(mb_hash, &decryption_keys)?;
        }

        self.try_emit_or_queue(
            MalachiteEvent::BlockProposal {
                height: mb.height,
                mb_hash,
            },
            last_advanced,
        )
        .await;
        Ok(())
    }

    async fn process_mb_finalized(
        &self,
        mb_hash: H256,
        cert: ethexe_malachite_core::CommitCertificate,
    ) -> Result<()> {
        if let Some(pool) = self.mempool.as_ref() {
            // Remove any finalized MB's proposed injected txs from the mempool.

            let compact = self
                .db
                .mb_compact_block(mb_hash)
                .with_context(|| format!("no CompactMb for {mb_hash}"))?;

            let operations = self
                .db
                .operations(compact.operations_hash)
                .with_context(|| format!("operations blob missing for block {mb_hash}"))?;

            let transactions = operations
                .iter()
                .filter_map(utils::operation_to_transaction)
                .collect::<Vec<_>>();
            if !transactions.is_empty() {
                pool.forget(&transactions).await;
            }
        }

        self.db
            .globals_mutate(|g| g.latest_finalized_mb_hash = mb_hash);

        // Retain shares belonging to another block
        self.decryption_shares.retain_block(mb_hash);

        let app_cert = CommitCertificate {
            height: cert.height,
            mb_hash,
            signatures: cert.signatures,
        };
        let last_advanced = self.db.mb_meta(mb_hash).last_advanced_eb;
        self.try_emit_or_queue(
            MalachiteEvent::BlockFinalized {
                cert: app_cert,
                height: cert.height,
                mb_hash,
            },
            last_advanced,
        )
        .await;

        Ok(())
    }

    async fn build_block_above(&self, parent_mb_hash: H256) -> Result<BlockPayload> {
        ensure!(
            self.mempool.is_some(),
            "build_block_above must not be called when node is not validator"
        );

        let decryption_keys = self
            .wait_for_shielded_tx_decryption_keys(parent_mb_hash)
            .await?;
        let parent_advanced = parent_mb_hash
            .is_zero()
            .then(H256::zero)
            .unwrap_or_else(|| self.db.mb_meta(parent_mb_hash).last_advanced_eb);

        let (advance, transactions) = if decryption_keys.is_some() {
            // Fast snapshot of proposable content. If no content propose block with decryption keys only.
            self.proposable_content_snapshot(parent_advanced).await?
        } else {
            self.wait_for_proposable_content(parent_advanced).await?
        };

        debug!(
            %parent_mb_hash,
            %parent_advanced,
            advance = ?advance,
            has_decryption_keys = decryption_keys.is_some(),
            transactions_count = transactions.len(),
            "build_block_above: proposable content resolved",
        );

        // (a) Per-tx validity. Each candidate tx from the mempool is
        // run through TxValidityChecker so we don't waste an MB
        // round-trip on a tx the participant would reject.
        let valid = if transactions.is_empty() {
            Vec::new()
        } else {
            let chain_head = *self.chain_head.latest_synced.read().await;
            let checker =
                TxValidityChecker::new_for_mb(self.db.clone(), chain_head, parent_mb_hash)?;
            let mut accepted = Vec::with_capacity(transactions.len());
            for tx in transactions {
                match checker.check_tx_validity(tx.as_ref())? {
                    TxValidity::Valid => accepted.push(tx),
                    reason => {
                        debug!(
                            tx_hash = %tx.as_ref().hash(),
                            ?reason,
                            "build_block_above: dropping transaction — fails TxValidity",
                        );
                    }
                }
            }
            accepted
        };

        let mut touched = match advance {
            Some(advanced_eb) => eb_touched_programs(&self.db, parent_advanced, advanced_eb)?,
            None => Default::default(),
        };
        if let Some(keys) = &decryption_keys {
            let UnshieldingOutput { unshielded, .. } =
                self.unshield_parent_transactions(parent_mb_hash, keys)?;
            touched.extend(
                unshielded
                    .iter()
                    .map(|(_, injected_tx)| injected_tx.data().destination),
            );
        }
        let initial_touched_count = touched.len();
        if initial_touched_count > MAX_TOUCHED_PROGRAMS_PER_MB as usize {
            // Producer can't shrink this — the EB events themselves
            // already exceed the cap. Drop injected txs and let the
            // MB advance the EB anyway so the chain progresses.
            error!(
                initial_touched_count,
                limit = MAX_TOUCHED_PROGRAMS_PER_MB,
                "build_block_above: EB events already exceed touched-programs cap; \
                 dropping all injected txs from this MB",
            );
        }

        // Cap the injected txs to stay within the remaining limits
        let mut size_counter: usize = 0;
        let mut capped: Vec<Transaction> = Vec::with_capacity(valid.len());
        for tx in valid {
            // Skip the whole loop body once initial touched > limit —
            // any injected tx would only push it further over.
            if initial_touched_count > MAX_TOUCHED_PROGRAMS_PER_MB as usize {
                break;
            }

            let tx_size = tx.encoded_size();
            if size_counter + tx_size > MAX_INJECTED_TRANSACTIONS_SIZE_PER_MB {
                // Skip the oversized tx but keep trying smaller subsequent ones.
                continue;
            }

            let destination = match &tx {
                Transaction::Injected(tx) => Some(tx.data().destination),
                Transaction::Shielded(_) => None,
            };
            if let Some(destination) = destination {
                if !touched.contains(&destination)
                    && touched.len() >= MAX_TOUCHED_PROGRAMS_PER_MB as usize
                {
                    // Adding this destination would breach the cap; skip.
                    continue;
                }

                touched.insert(destination);
            }

            size_counter += tx_size;
            capped.push(tx);
        }

        let mut operations = Vec::with_capacity(capped.len() + 3);
        if let Some(block_hash) = advance {
            operations.push(Operation::AdvanceTillEthereumBlock { block_hash });
        }
        if let Some(keys) = decryption_keys {
            operations.push(Operation::DecryptionKeys(keys));
        }
        for tx in capped {
            operations.push(utils::transaction_to_operation(tx));
        }
        operations.push(Operation::ProgressTasks);
        operations.push(Operation::ProcessQueuesV3 {
            gas_allowance: self.cfg.gas_allowance,
        });

        let bytes = Operations::new(operations).encode();
        let len = bytes.len();
        BlockPayload::try_from(bytes).map_err(|_| {
            anyhow!("built block payload exceeds {MAX_BLOCK_PAYLOAD_BYTES}-byte cap (got {len})")
        })
    }

    async fn validate_block_above(
        &self,
        parent_hash: H256,
        payload: &BlockPayload,
    ) -> Result<Acceptance<(), String>> {
        let payload = match Operations::decode_all(&mut payload.as_ref()) {
            Ok(payload) => payload,
            Err(e) => {
                return Ok(Acceptance::Rejected(format!(
                    "undecodable block payload: {e}"
                )));
            }
        };

        // Reject operations not allowed at this protocol version (e.g. the
        // deprecated `ProcessQueues` v1 with the old mailbox validity).
        for op in payload.iter() {
            match op {
                Operation::AdvanceTillEthereumBlock { .. }
                | Operation::ProgressTasks
                | Operation::ProcessQueuesV3 { .. }
                | Operation::Injected(_)
                | Operation::Shielded(_)
                | Operation::DecryptionKeys(_) => {
                    // Known and allowed.
                }
                op => {
                    return Ok(Acceptance::Rejected(format!(
                        "deprecated operation in proposed MB: {op:?}"
                    )));
                }
            }
        }

        let mut iter = payload.iter();
        let mut next = iter.next();

        let advance: Option<H256> =
            if let Some(Operation::AdvanceTillEthereumBlock { block_hash }) = next {
                let h = *block_hash;
                next = iter.next();
                Some(h)
            } else {
                None
            };

        let decryption_keys = if let Some(Operation::DecryptionKeys(keys)) = next {
            let keys = Some(keys);
            next = iter.next();
            keys
        } else {
            None
        };

        // Skip injected and shielded txs for now, check them a little later
        while matches!(next, Some(Operation::Injected(_) | Operation::Shielded(_))) {
            next = iter.next();
        }

        let Some(Operation::ProgressTasks) = next else {
            return Ok(Acceptance::Rejected(format!(
                "MB shape violation — expected `ProgressTasks` bookend, got {:?}",
                next.map(|t| t.tag())
            )));
        };

        let Some(Operation::ProcessQueuesV3 { gas_allowance }) = iter.next() else {
            return Ok(Acceptance::Rejected(
                "MB shape violation — expected `ProcessQueuesV3` bookend".to_string(),
            ));
        };

        if *gas_allowance > crate::MalachiteServiceConfig::DEFAULT_GAS_ALLOWANCE {
            return Ok(Acceptance::Rejected(format!(
                "ProcessQueuesV3.gas_allowance {gas_allowance} exceeds protocol cap {}",
                crate::MalachiteServiceConfig::DEFAULT_GAS_ALLOWANCE
            )));
        }

        if iter.next().is_some() {
            return Ok(Acceptance::Rejected(
                "MB has extra operations after the `ProcessQueuesV3` bookend".to_string(),
            ));
        }

        // TODO: #5477 extract a shared `check_eb_advance` helper so this
        //       validator path and `find_eb_candidate_for_advancing` on the
        //       producer side stay in lockstep through future refactors.
        // TODO: #5479 emit `malachite_validate_abstain_total{reason=...}` at
        //       each early-return below so operators can tune
        //       `post_quarantine_delay` from observability rather than logs.

        // Take latest synced EB as the reference point
        // for all the quarantine and transactions checks below
        let chain_head = *self.chain_head.latest_synced.read().await;

        // Advanced block quarantine checks
        if let Some(advance) = advance {
            let Some(advance) = self.db.block_simple_data(advance) else {
                return Ok(Acceptance::Rejected(format!(
                    "advance EB {advance} not found in local DB"
                )));
            };

            if advance
                .header
                .height
                .saturating_add(self.cfg.canonical_quarantine as u32)
                > chain_head.header.height
            {
                return Ok(Acceptance::Rejected(format!(
                    "advance EB {advance} does not pass quarantine against local chain head {chain_head}",
                )));
            }

            let parent_advanced = parent_hash
                .is_zero()
                .then(H256::zero)
                .unwrap_or_else(|| self.db.mb_meta(parent_hash).last_advanced_eb);
            let start_block_hash = self.db.globals().start_block_hash;
            match quarantine::is_strict_descendant_of(
                &self.db,
                advance,
                parent_advanced,
                start_block_hash,
            ) {
                Ok(Acceptance::Accepted(())) => {}
                Ok(Acceptance::Rejected(reason)) => {
                    return Ok(Acceptance::Rejected(format!(
                        "advance {advance} is not a strict descendant of parent_advanced {parent_advanced}: {reason}"
                    )));
                }
                Err(e) => return Err(e),
            }
        }

        // let Some(chain_head) = chain_head_snapshot else {
        //     let has_injected = operations
        //         .iter()
        //         .any(|tx| utils::operation_to_transaction(tx).is_some());
        //     if has_injected {
        //         warn!("validate: MB carries injected txs but no local chain head — abstaining");
        //         return Ok(false);
        //     }
        //     return Ok(true);
        // };

        // Validate injected txs
        if payload
            .iter()
            .any(|op| utils::operation_to_transaction(op).is_some())
        {
            let checker = TxValidityChecker::new_for_mb(self.db.clone(), chain_head, parent_hash)?;
            for op in payload.iter() {
                let Some(transaction) = utils::operation_to_transaction(op) else {
                    continue;
                };
                match checker.check_tx_validity(transaction)? {
                    TxValidity::Valid => {}
                    reason => {
                        return Ok(Acceptance::Rejected(format!(
                            "transaction {} fails TxValidity: {reason:?}",
                            transaction.hash()
                        )));
                    }
                }
            }
        }

        let parent_advanced = parent_hash
            .is_zero()
            .then(H256::zero)
            .unwrap_or_else(|| self.db.mb_meta(parent_hash).last_advanced_eb);
        let mut touched = match advance {
            Some(advanced_eb) => eb_touched_programs(&self.db, parent_advanced, advanced_eb)?,
            None => Default::default(),
        };
        if let Some(keys) = decryption_keys {
            let UnshieldingOutput { unshielded, .. } =
                self.unshield_parent_transactions(parent_hash, keys)?;
            touched.extend(
                unshielded
                    .iter()
                    .map(|(_, injected_tx)| injected_tx.data().destination),
            );
        }
        let limit = touched.len().max(MAX_TOUCHED_PROGRAMS_PER_MB as usize);
        for op in payload.iter() {
            match op {
                Operation::Injected(signed) => {
                    touched.insert(signed.data().destination);
                }
                Operation::Shielded(_) => {}
                _ => {}
            }
        }
        if touched.len() > limit {
            return Ok(Acceptance::Rejected(format!(
                "MB touches too many programs: {} > limit {limit}",
                touched.len()
            )));
        }

        Ok(Acceptance::Accepted(()))
    }
}

impl EthexeExternalities {
    /// Check whether the prerequisite EB is prepared in local DB.
    /// Zero hash is a special case that always passes.
    fn prerequisite_satisfied(&self, prerequisite: H256) -> bool {
        use ethexe_common::db::BlockMetaStorageRO;
        prerequisite.is_zero() || self.db.block_meta(prerequisite).prepared
    }

    /// Send event immediately if prerequisite is satisfied, otherwise queue it for later emission.
    pub(crate) async fn try_emit_or_queue(&self, event: MalachiteEvent, prerequisite: H256) {
        let mut queue = self.pending_events.write().await;
        if queue.is_empty() && self.prerequisite_satisfied(prerequisite) {
            // Channel receiver dropped only on shutdown — best-effort.
            let _ = self.event_tx.send(Ok(event));
        } else {
            queue.push_back(PendingEvent {
                event,
                prerequisite,
            });
        }
    }

    /// Check the pending events queue and release any events whose prerequisites are now satisfied.
    pub(crate) async fn drain_pending_events(&self) {
        let mut queue = self.pending_events.write().await;
        while let Some(front) = queue.front() {
            if !self.prerequisite_satisfied(front.prerequisite) {
                break;
            }
            let entry = queue.pop_front().expect("just peeked");
            let _ = self.event_tx.send(Ok(entry.event));
        }
    }

    // Wait for either a new EB candidate past quarantine
    // or any suitable injected tx to include in the next proposal.
    async fn wait_for_proposable_content(
        &self,
        prev_advanced_eb_hash: H256,
    ) -> Result<(Option<H256>, Vec<Transaction>)> {
        let mempool = self
            .mempool
            .as_ref()
            .context("must never wait for proposable content when not a validator")?;
        loop {
            let chain_head_notified = self.chain_head.notify.notified();
            tokio::pin!(chain_head_notified);
            chain_head_notified.as_mut().enable();

            let (advance, transactions) = self
                .proposable_content_snapshot(prev_advanced_eb_hash)
                .await?;

            if advance.is_some() || !transactions.is_empty() {
                return Ok((advance, transactions));
            }

            tokio::select! {
                biased;
                _ = chain_head_notified => {}
                _ = mempool.wait_for_new_tx() => {}
            }
        }
    }

    /// Read currently available producer inputs without waiting for
    /// any of them to appear.
    ///
    /// This function called in [`Self::wait_for_proposable_content`] on each poll
    /// iteration, and from [`Self::build_block_above`] when decryption keys are already ready.
    async fn proposable_content_snapshot(
        &self,
        prev_advanced_eb_hash: H256,
    ) -> Result<(Option<H256>, Vec<Transaction>)> {
        let advance = self
            .find_eb_candidate_for_advancing(prev_advanced_eb_hash)
            .await?;

        let chain_head = *self.chain_head.latest_synced.read().await;
        let Some(mempool) = self.mempool.as_ref() else {
            anyhow::bail!("must never call wait_for_proposable_content when not a validator");
        };
        let transactions = mempool.fetch(chain_head).await;

        Ok((advance, transactions))
    }

    /// Wait until every shielded transaction in the parent has enough verified
    /// shares, then reconstruct one shared secret per transaction.
    async fn wait_for_shielded_tx_decryption_keys(
        &self,
        parent_mb_hash: H256,
    ) -> Result<Option<DecryptionKeys>> {
        if parent_mb_hash.is_zero() {
            return Ok(None);
        }
        let Some(compact) = self.db.mb_compact_block(parent_mb_hash) else {
            bail!("compact block not found for block with hash={parent_mb_hash}")
        };

        let Some(operations) = self.db.operations(compact.operations_hash) else {
            bail!(
                "operations not found for block with hash={parent_mb_hash}, op_hash={}",
                compact.operations_hash
            )
        };

        let mut pending = operations
            .iter()
            .filter_map(|op| op.as_shielded().map(|tx| tx.data().to_hash()))
            .collect::<HashSet<_>>();

        // No shielded transactions in previous block, do not need to wait for decryption shares.
        if pending.is_empty() {
            return Ok(None);
        }

        let Some(ctx) = self.tdec_ctx.as_ref() else {
            bail!("block producer has no threshold-decryption context")
        };

        let threshold = ctx.threshold.get();
        if threshold > ctx.contexts.len() {
            bail!(
                "invalid threshold-decryption context: threshold={threshold}, participants={}",
                ctx.contexts.len()
            );
        }

        let mut keys = DecryptionKeys::default();
        while !pending.is_empty() {
            pending.retain(|tx_hash| {
                let Some(selected) =
                    self.decryption_shares
                        .threshold_shares(parent_mb_hash, *tx_hash, threshold)
                else {
                    return true;
                };

                let domains = selected
                    .iter()
                    .map(|(validator, _)| {
                        ctx.contexts
                            .get(validator)
                            .expect("stored share has a validator context")
                            .domain
                    })
                    .collect::<Vec<_>>();
                let shares = selected
                    .into_iter()
                    .map(|(_, share)| share)
                    .collect::<Vec<DecryptionShareSimple>>();
                let coefficients = prepare_combine_simple::<gear_tdec::bls12_381::E>(&domains);
                keys.insert(
                    *tx_hash,
                    share_combine_simple::<gear_tdec::bls12_381::E>(&shares, &coefficients),
                );
                false
            });

            if !pending.is_empty() {
                self.decryption_shares.notified().await;
            }
        }

        Ok(Some(keys))
    }

    fn receive_decryption_shares(
        &self,
        mb_hash: H256,
        sender: Address,
        shares: &[ShieldedTxDecryptionShare],
    ) -> Acceptance<(), String> {
        let Some(context) = self.tdec_ctx.as_ref() else {
            return Acceptance::Rejected("local TDEC context is unavailable".into());
        };

        let Some(compact) = self.db.mb_compact_block(mb_hash) else {
            return Acceptance::Rejected(format!("unknown MB {mb_hash}"));
        };
        let Some(operations) = self.db.operations(compact.operations_hash) else {
            return Acceptance::Rejected(format!(
                "operations {} for MB {mb_hash} are missing",
                compact.operations_hash
            ));
        };

        let Some(participant_context) = context.contexts.get(&sender) else {
            return Acceptance::Rejected(format!("unknown TDEC participant {sender}"));
        };
        let transactions = operations
            .iter()
            .filter_map(|op| op.as_shielded().map(|signed| signed.data()))
            .map(|tx| (tx.to_hash(), tx))
            .collect::<HashMap<_, _>>();

        let mut seen = HashSet::with_capacity(shares.len());
        for message_share in shares {
            if !seen.insert(message_share.tx_hash) {
                return Acceptance::Rejected(format!(
                    "duplicate decryption share for transaction {}",
                    message_share.tx_hash.inner()
                ));
            }
            let Some(transaction) = transactions.get(&message_share.tx_hash) else {
                return Acceptance::Rejected(format!(
                    "decryption share for transaction {} outside MB {mb_hash}",
                    message_share.tx_hash.inner()
                ));
            };
            match self.decryption_shares.insert(
                mb_hash,
                message_share.tx_hash,
                sender,
                participant_context,
                transaction,
                message_share.share.clone(),
            ) {
                InsertOutcome::Inserted | InsertOutcome::Duplicate => {}
                InsertOutcome::InvalidShare => {
                    return Acceptance::Rejected("invalid decryption share".into());
                }
                InsertOutcome::Equivocation => {
                    return Acceptance::Rejected("conflicting decryption share".into());
                }
                InsertOutcome::UnknownBlock | InsertOutcome::UnknownTransaction => {
                    return Acceptance::Rejected("unknown MB or transaction".into());
                }
            }
        }
        Acceptance::Accepted(())
    }

    fn provide_decryption_shares(
        &self,
        mb_hash: H256,
        tdec_ctx: &PublicDecryptionContext,
        my_address: Address,
        transactions: &[&ShieldedTransaction],
    ) -> Vec<ShieldedTxDecryptionShare> {
        let mut shares = Vec::with_capacity(transactions.len());
        for tx in transactions {
            let Ok(share) =
                self.tdec_store
                    .create_share(tdec_ctx, &tx.ciphertext.header(), tx.aad.as_ref())
            else {
                continue;
            };
            let tx_hash = tx.to_hash();
            let outcome = self.decryption_shares.insert(
                mb_hash,
                tx_hash,
                my_address,
                tdec_ctx,
                tx,
                share.clone(),
            );
            debug_assert!(matches!(
                outcome,
                InsertOutcome::Inserted | InsertOutcome::Duplicate
            ));
            shares.push(ShieldedTxDecryptionShare { tx_hash, share });
        }

        shares
    }

    fn process_unshielding(&self, mb_hash: H256, decryption_keys: &DecryptionKeys) -> Result<()> {
        let compact = self
            .db
            .mb_compact_block(mb_hash)
            .context("process_unshielding: no compact for {mb_hash}")?;

        let UnshieldingOutput {
            unshielded: unshielded_with_hashes,
            not_unshielded,
        } = self.unshield_parent_transactions(compact.parent, decryption_keys)?;

        let unshielded_hash_mapping = unshielded_with_hashes
            .iter()
            .map(|(tx_hash, tx)| (*tx_hash, tx.data().to_hash()))
            .collect();

        let unshielded = unshielded_with_hashes
            .into_iter()
            .map(|(_, tx)| tx)
            .collect();

        self.db.set_mb_unshielded_txs(mb_hash, unshielded);

        let _ = self.event_tx.send(Ok(MalachiteEvent::UnshieldingOutput {
            mb_hash,
            unshielded_hash_mapping,
            not_unshielded,
        }));

        Ok(())
    }

    fn unshield_parent_transactions(
        &self,
        parent_mb_hash: H256,
        decryption_keys: &DecryptionKeys,
    ) -> Result<UnshieldingOutput> {
        if parent_mb_hash.is_zero() || decryption_keys.is_empty() {
            return Ok(UnshieldingOutput::default());
        }

        let compact = self.db.mb_compact_block(parent_mb_hash).ok_or_else(|| {
            anyhow!("unshield_parent_transactions: no CompactMb for parent {parent_mb_hash}")
        })?;
        let operations = self.db.operations(compact.operations_hash).ok_or_else(|| {
            anyhow!(
                "unshield_parent_transactions: operations blob {} missing for parent {parent_mb_hash}",
                compact.operations_hash
            )
        })?;

        let mut output = UnshieldingOutput::default();
        for tx in operations.into_iter().filter_map(Operation::into_shielded) {
            let tx_hash = tx.data().to_hash();
            match decryption_keys.get(&tx_hash) {
                Some(shared_key) => {
                    match tx.into_verified().try_map(|tx| tx.unshield(shared_key)) {
                        Ok(injected_tx) => output.unshielded.push((tx_hash, injected_tx)),
                        Err(_err) => output.not_unshielded.push(PurgedTransaction {
                            tx_hash: TransactionHash::Right(tx_hash),
                            reason: TransactionPurgedReason::DecryptionFailed,
                        }),
                    }
                }
                None => output.not_unshielded.push(PurgedTransaction {
                    tx_hash: TransactionHash::Right(tx_hash),
                    reason: TransactionPurgedReason::DecryptionFailed,
                }),
            }
        }

        Ok(output)
    }

    // Find an EB candidate that can be advanced to according to the current chain head:
    // 1. Should pass quarantine with post quarantine delay against the latest synced EB.
    // 2. Should be a strict descendant of the previously advanced EB.
    async fn find_eb_candidate_for_advancing(&self, parent_advance: H256) -> Result<Option<H256>> {
        let chain_head = *self.chain_head.latest_synced.read().await;
        let start = self.db.globals().start_block_hash;
        let total_depth = self.cfg.canonical_quarantine as u32 + self.cfg.post_quarantine_delay;

        let candidate = match quarantine::anchor(&self.db, chain_head, total_depth, start) {
            Ok(Some(c)) => c,
            Ok(None) => {
                trace!("anchor lookup reached start block; skipping advance");
                return Ok(None);
            }
            Err(e) => return Err(anyhow!("quarantine anchor lookup failed: {e}")),
        };

        if candidate.hash == parent_advance {
            // No new EB past quarantine since the parent's advance.
            return Ok(None);
        }

        match quarantine::is_strict_descendant_of(&self.db, candidate, parent_advance, start) {
            Ok(Acceptance::Accepted(())) => Ok(Some(candidate.hash)),
            Ok(Acceptance::Rejected(reason)) => {
                warn!(
                    reason = %reason,
                    candidate = %candidate,
                    parent_advanced = %parent_advance,
                    "quarantine-passed EB is not a canonical descendant of \
                     parent's last_advanced_eb — skipping AdvanceTillEthereumBlock"
                );
                Ok(None)
            }
            Err(e) => Err(e).context("quarantine descendant check failed"),
        }
    }
}

mod utils {
    use ethexe_common::{
        injected::{Transaction, TransactionRef},
        malachite::Operation,
    };
    /// Optimization for reducing `clone` operation for potentially large transactions.
    pub(crate) fn operation_to_transaction(operation: &Operation) -> Option<TransactionRef<'_>> {
        match operation {
            Operation::Injected(tx) => Some(TransactionRef::Injected(tx)),
            Operation::Shielded(tx) => Some(TransactionRef::Shielded(tx)),
            _ => None,
        }
    }

    pub(crate) fn transaction_to_operation(transaction: Transaction) -> Operation {
        match transaction {
            Transaction::Injected(tx) => Operation::Injected(tx),
            Transaction::Shielded(tx) => Operation::Shielded(tx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mempool::EmptyMempool;
    use anyhow::Context;
    use ethexe_common::{
        BlockHeader, SimpleBlockData,
        db::{BlockMetaStorageRW, OnChainStorageRW},
        injected::{PurgedTransaction, SignedInjectedTransaction, TransactionRef},
    };
    use gear_tdec::{
        bls12_381::{DkgPublicKey, E as Bls12_381},
        rand_utils::test_rng,
    };
    use tokio::sync::{Notify, mpsc};

    fn make_chain_head() -> Arc<ChainHead> {
        Arc::new(ChainHead {
            latest: RwLock::new(SimpleBlockData::default()),
            latest_synced: RwLock::new(SimpleBlockData::default()),
            notify: Notify::new(),
        })
    }

    async fn set_head(ext: &EthexeExternalities, head: SimpleBlockData) {
        *ext.chain_head.latest.write().await = head;
        *ext.chain_head.latest_synced.write().await = head;
    }

    fn to_payload(bytes: Vec<u8>) -> BlockPayload {
        BlockPayload::try_from(bytes).expect("test payload within size cap")
    }

    impl EthexeExternalities {
        /// Test-only convenience wrapper: SCALE-encode `ops` into the
        /// size-capped block payload, then run the standard validate path.
        /// Mirrors the producer-side encoding step the inner core service
        /// applies to whatever `build_block_above` returns.
        async fn validate_operations(&self, parent: H256, ops: Operations) -> Result<bool> {
            self.validate_block_above(parent, &to_payload(ops.encode()))
                .await
                .map(|acceptance| acceptance.is_accepted())
        }

        /// Test-only inverse of [`Self::validate_operations`]: run the
        /// standard build path and decode its payload bytes back into the
        /// application's [`Operations`] shape.
        async fn build_operations(&self, parent: H256) -> Result<Operations> {
            Operations::decode_all(&mut self.build_block_above(parent).await?.as_ref())
                .context("operations decoding error")
        }
    }

    /// Build a small ethexe `Database`-backed externalities + the
    /// matching event receiver. No ethexe-malachite-core or libp2p involved —
    /// callbacks are invoked directly so we can assert on side
    /// effects deterministically.
    fn make_externalities(
        db: Database,
    ) -> (
        EthexeExternalities,
        mpsc::UnboundedReceiver<Result<MalachiteEvent>>,
    ) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let ext = EthexeExternalities {
            db,
            tdec_ctx: None,
            tdec_store: TdecKeyStore::memory(),
            mempool: Some(Arc::new(EmptyMempool)),
            chain_head: make_chain_head(),
            decryption_shares: Arc::new(DecryptionSharesStore::new()),
            event_tx,
            pending_events: RwLock::new(VecDeque::new()),
            cfg: ExternalitiesConfig {
                gas_allowance: 1_000_000,
                canonical_quarantine: 0,
                post_quarantine_delay: 0,
            },
        };
        (ext, event_rx)
    }

    /// Do threshold decryption setup for a single validator.
    fn single_validator_tdec_setup() -> (MalachiteTdecContext, TdecKeyStore, DkgPublicKey) {
        let validator_key = ethexe_common::PrivateKey::random();
        let validator_public_key = validator_key.public_key();
        let dealer = gear_tdec::deal::<Bls12_381>(1, 1, &mut test_rng());
        let private_context = dealer
            .private_contexts
            .into_iter()
            .next()
            .expect("single-validator dealer output must contain a private context");
        let public_context = private_context
            .public_decryption_contexts
            .first()
            .cloned()
            .expect("single-validator dealer output must contain a public context");

        let key_store = TdecKeyStore::memory();
        key_store
            .import_decryption_key(private_context.validator_decryption_key)
            .expect("dealer TDEC key must be importable");

        (
            MalachiteTdecContext {
                threshold: std::num::NonZeroUsize::new(1).expect("threshold is non-zero"),
                my_context: public_context.clone(),
                contexts: HashMap::from([(validator_public_key.to_address(), public_context)]),
            },
            key_store,
            dealer.public_key,
        )
    }

    /// Build an [`Operations`] list for unit tests.
    ///
    /// The `salt` byte is encoded as the number of leading
    /// `ProgressTasks` placeholders, which gives each block a unique
    /// hash without dragging an extraneous `AdvanceTillEthereumBlock`
    /// through the test (the `last_advanced_eb_propagates` case
    /// would otherwise see an unintended advance).
    fn payload(advance: Option<H256>, salt: u8) -> Operations {
        let mut txs = Vec::with_capacity(salt as usize + 3);
        if let Some(eth) = advance {
            txs.push(Operation::AdvanceTillEthereumBlock { block_hash: eth });
        }
        // Salt = number of repeated ProgressTasks. Salt 0 is illegal
        // (collides with another zero-salt block); the helpers below
        // always pass salt >= 1.
        for _ in 0..(salt.max(1)) {
            txs.push(Operation::ProgressTasks);
        }
        txs.push(Operation::ProcessQueuesV3 { gas_allowance: 0 });
        Operations::new(txs)
    }

    fn wrap(payload: Operations, height: u64, parent_hash: H256) -> Block {
        Block::new(parent_hash, height, to_payload(payload.encode()))
    }

    fn fake_cert(height: u64) -> ethexe_malachite_core::CommitCertificate {
        ethexe_malachite_core::CommitCertificate {
            height,
            block_hash: H256::zero(), // unused by process_mb_finalized
            signatures: vec![vec![0u8; 64]],
        }
    }

    /// `process_mb_proposal` populates `mb_block`, `mb_meta` (height,
    /// parent_mb_hash, last_advanced_eb, synced=true) and the
    /// height index, then emits a `BlockProposal`.
    #[tokio::test]
    async fn process_mb_proposal_populates_db_and_emits_event() {
        use ethexe_common::db::{GlobalsStorageRO, MbStorageRO};
        let db = Database::memory();
        let (ext, mut rx) = make_externalities(db.clone());
        let p = payload(None, 1);
        let block = wrap(p.clone(), 1, H256::zero());
        let mb_hash = block.hash();
        ext.process_mb_proposal(mb_hash, block).await.unwrap();

        let compact = db.mb_compact_block(mb_hash).expect("CompactMb saved");
        assert_eq!(compact.height, 1);
        assert_eq!(compact.parent, H256::zero());
        let txs = db
            .operations(compact.operations_hash)
            .expect("operations in CAS");
        assert_eq!(txs, p);

        match rx.try_recv().expect("event").expect("ok") {
            MalachiteEvent::BlockProposal {
                height,
                mb_hash: proposed,
            } => {
                assert_eq!(height, 1);
                assert_eq!(proposed, mb_hash);
                let _ = p;
            }
            other => panic!("expected BlockProposal, got {other:?}"),
        }

        // Globals not advanced by save — finalize is what does that.
        assert!(db.globals().latest_finalized_mb_hash.is_zero());
    }

    #[tokio::test]
    async fn process_mb_proposal_unshields_parent_transactions() {
        use ethexe_common::{
            SignedMessage,
            db::MbStorageRO,
            injected::{InjectedTransaction, TransactionHash},
        };
        use gprimitives::ActorId;

        let db = Database::memory();
        let (mut ext, mut rx) = make_externalities(db.clone());
        let (tdec_ctx, tdec_store, dkg_public_key) = single_validator_tdec_setup();
        ext.tdec_ctx = Some(tdec_ctx);
        ext.tdec_store = tdec_store;

        let injected = InjectedTransaction {
            destination: ActorId::from([1; 32]),
            payload: vec![1, 2, 3].try_into().unwrap(),
            value: 0,
            reference_block: H256::zero(),
            salt: vec![7; 32].try_into().unwrap(),
        };
        let shielded = injected
            .clone()
            .shield(&dkg_public_key, &mut test_rng())
            .expect("test shielding must succeed");
        let shielded_hash = shielded.to_hash();
        let signed_shielded =
            SignedMessage::create(ethexe_common::PrivateKey::random(), shielded).unwrap();

        let parent_payload = Operations::new(vec![
            Operation::Shielded(signed_shielded),
            Operation::ProgressTasks,
            Operation::ProcessQueuesV3 { gas_allowance: 0 },
        ]);
        let parent = wrap(parent_payload, 1, H256::zero());
        let parent_hash = parent.hash();
        ext.process_mb_proposal(parent_hash, parent).await.unwrap();
        let _ = rx.recv().await.expect("parent proposal").expect("ok");
        assert!(ext.extend_vote(parent_hash).await.unwrap().is_some());

        let child_payload = ext
            .build_operations(parent_hash)
            .await
            .expect("single-validator shares should produce decryption keys");
        let child = wrap(child_payload, 2, parent_hash);
        let child_hash = child.hash();
        ext.process_mb_proposal(child_hash, child).await.unwrap();

        let unshielded = db.mb_unshielded_txs(child_hash);
        assert_eq!(unshielded.len(), 1);
        assert_eq!(unshielded[0].data(), &injected);

        match rx.try_recv().expect("unshielding event").expect("ok") {
            MalachiteEvent::UnshieldingOutput {
                mb_hash,
                unshielded_hash_mapping,
                not_unshielded,
            } => {
                assert_eq!(mb_hash, child_hash);
                assert_eq!(
                    unshielded_hash_mapping,
                    vec![(shielded_hash, injected.to_hash())]
                );
                assert!(not_unshielded.is_empty());
            }
            other => panic!("expected UnshieldingOutput, got {other:?}"),
        }

        assert_eq!(
            TransactionHash::Left(unshielded[0].data().to_hash()),
            TransactionHash::Left(injected.to_hash())
        );
    }

    /// `process_mb_finalized` reads the [`CompactMb`] +
    /// operations blob keyed by the consensus envelope hash,
    /// advances `globals.latest_finalized_mb_hash`, and emits a
    /// `BlockFinalized`.
    #[tokio::test]
    async fn finalize_advances_globals_and_emits_event() {
        use ethexe_common::db::GlobalsStorageRO;
        let db = Database::memory();
        let (ext, mut rx) = make_externalities(db.clone());
        let p = payload(None, 5);
        let block = wrap(p.clone(), 1, H256::zero());
        let mb_hash = block.hash();
        ext.process_mb_proposal(mb_hash, block).await.unwrap();
        let _ = rx.recv().await; // BlockProposal
        ext.process_mb_finalized(mb_hash, fake_cert(1))
            .await
            .unwrap();
        assert_eq!(db.globals().latest_finalized_mb_hash, mb_hash);
        match rx.try_recv().expect("event").expect("ok") {
            MalachiteEvent::BlockFinalized {
                cert,
                height,
                mb_hash: finalized,
            } => {
                assert_eq!(height, 1);
                assert_eq!(mb_hash, finalized);
                assert_eq!(cert.height, 1);
                assert_eq!(cert.mb_hash, mb_hash);
                let _ = p;
            }
            other => panic!("expected BlockFinalized, got {other:?}"),
        }
    }

    /// Crash-recovery: build externalities A on a fresh DB, save +
    /// finalize K MBs, drop A, build externalities B on the same DB.
    /// B sees the persisted globals and `CompactMb` chain; the
    /// next `process_mb_proposal` correctly chains off the previous head.
    #[tokio::test]
    async fn restart_continues_from_persisted_chain() {
        use ethexe_common::db::{GlobalsStorageRO, MbStorageRO};
        let db = Database::memory();
        let (ext_a, mut rx_a) = make_externalities(db.clone());

        let mut chain: Vec<(H256, Operations)> = Vec::new();
        let mut parent = H256::zero();
        for i in 1..=3u64 {
            let p = payload(None, i as u8);
            let block = wrap(p.clone(), i, parent);
            let mb_hash = block.hash();
            ext_a.process_mb_proposal(mb_hash, block).await.unwrap();
            ext_a
                .process_mb_finalized(mb_hash, fake_cert(i))
                .await
                .unwrap();
            chain.push((mb_hash, p));
            parent = mb_hash;
        }
        // Drain events so the channel doesn't hold stale references.
        while rx_a.try_recv().is_ok() {}
        drop(ext_a);
        drop(rx_a);

        // After "restart" — fresh externalities on the SAME DB.
        let (ext_b, mut rx_b) = make_externalities(db.clone());

        // Pre-restart pointers must survive.
        let last_pre = chain.last().unwrap().0;
        assert_eq!(db.globals().latest_finalized_mb_hash, last_pre);
        for (i, (mb_hash, _)) in chain.iter().enumerate() {
            let compact = db.mb_compact_block(*mb_hash).expect("compact");
            assert_eq!(compact.height, (i + 1) as u64);
            let expected_parent = if i == 0 { H256::zero() } else { chain[i - 1].0 };
            assert_eq!(compact.parent, expected_parent);
        }

        // Save + finalize MB at height 4 chained off the head — the
        // parent resolution must see the height-3 record left by the
        // previous run.
        let p4 = payload(None, 99);
        let block4 = wrap(p4.clone(), 4, last_pre);
        let mb4 = block4.hash();
        ext_b.process_mb_proposal(mb4, block4).await.unwrap();
        let _ = rx_b.recv().await; // proposal
        ext_b.process_mb_finalized(mb4, fake_cert(4)).await.unwrap();
        assert_eq!(db.mb_compact_block(mb4).unwrap().parent, last_pre);
        assert_eq!(db.globals().latest_finalized_mb_hash, mb4);
    }

    /// `last_advanced_eb` is propagated forward: an MB without an
    /// `AdvanceTillEthereumBlock` inherits the parent's value; an MB
    /// with one resets it.
    #[tokio::test]
    async fn last_advanced_eb_propagates() {
        use ethexe_common::db::MbStorageRO;
        let db = Database::memory();
        let (ext, mut rx) = make_externalities(db.clone());

        let mut chain: Vec<H256> = Vec::new();
        let mut parent = H256::zero();
        let payloads = [
            payload(None, 1),
            payload(Some(H256::repeat_byte(0xAB)), 2),
            payload(None, 3),
        ];
        for (i, p) in payloads.iter().enumerate() {
            let height = (i + 1) as u64;
            let block = wrap(p.clone(), height, parent);
            let mb_hash = block.hash();
            ext.process_mb_proposal(mb_hash, block).await.unwrap();
            ext.process_mb_finalized(mb_hash, fake_cert(height))
                .await
                .unwrap();
            chain.push(mb_hash);
            parent = mb_hash;
        }
        while rx.try_recv().is_ok() {}

        assert!(db.mb_meta(chain[0]).last_advanced_eb.is_zero());
        assert_eq!(
            db.mb_meta(chain[1]).last_advanced_eb,
            H256::repeat_byte(0xAB),
            "h2 should anchor to its own AdvanceTillEthereumBlock"
        );
        assert_eq!(
            db.mb_meta(chain[2]).last_advanced_eb,
            H256::repeat_byte(0xAB),
            "h3 inherits h2's anchor"
        );
    }

    /// `validate_block_above` catches double-`AdvanceTillEthereumBlock`
    /// proposals. The second `Advance` lands where `Injected*` /
    /// `ProgressTasks` would be expected, so the shape walk rejects it.
    #[tokio::test]
    async fn validate_rejects_two_advances() {
        let db = Database::memory();
        let (ext, _rx) = make_externalities(db.clone());
        let payload = Operations::new(vec![
            Operation::AdvanceTillEthereumBlock {
                block_hash: H256::repeat_byte(0xAA),
            },
            Operation::AdvanceTillEthereumBlock {
                block_hash: H256::repeat_byte(0xBB),
            },
            Operation::ProgressTasks,
            Operation::ProcessQueuesV3 { gas_allowance: 0 },
        ]);
        assert!(
            !ext.validate_operations(H256::zero(), payload)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn validate_soft_rejects_trailing_garbage() {
        // `decode_all` rejects bytes left over after a well-formed
        // `Operations` list, so a padded payload is voted nil (not crashed).
        let db = Database::memory();
        let (ext, _rx) = make_externalities(db.clone());
        let mut bytes = payload(None, 1).encode();
        bytes.extend_from_slice(&[0u8; 16]);
        assert!(
            ext.validate_block_above(H256::zero(), &to_payload(bytes))
                .await
                .unwrap()
                .is_rejected()
        );
    }

    #[tokio::test]
    async fn process_mb_proposal_errors_on_undecodable_payload() {
        // An undecodable payload makes the callback surface an error; the
        // engine then logs it and drops the value (it is not ingested).
        let db = Database::memory();
        let (ext, _rx) = make_externalities(db.clone());
        let block = Block::new(H256::zero(), 1, to_payload(vec![0xff, 0xff, 0xff, 0xff]));
        let mb_hash = block.hash();
        assert!(ext.process_mb_proposal(mb_hash, block).await.is_err());
    }

    #[tokio::test]
    async fn validate_abstains_without_chain_head() {
        // Full-shape MB with one AdvanceTillEthereumBlock + no observer
        // head yet — the application can't verify the candidate's
        // quarantine status, so the vote is `Ok(false)` rather than `Err`.
        let db = Database::memory();
        let (ext, _rx) = make_externalities(db.clone());
        let payload = Operations::new(vec![
            Operation::AdvanceTillEthereumBlock {
                block_hash: H256::repeat_byte(0xCC),
            },
            Operation::ProgressTasks,
            Operation::ProcessQueuesV3 { gas_allowance: 0 },
        ]);
        assert!(
            !ext.validate_operations(H256::zero(), payload)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn validate_accepts_quarantine_passed_advance() {
        // canonical_quarantine = 0 in `make_externalities`, so any
        // ancestor of `head` in the local DB clears quarantine.
        let db = Database::memory();
        // The advance walk resolves the genesis MB's parent (the zero hash);
        // seed it as a computed ancestor exactly as `initialize_empty_db` does.
        ethexe_common::mock::seed_genesis_zero_mb(&db);
        let chain_hashes = {
            let mut hashes = Vec::with_capacity(3);
            let mut parent = H256::zero();
            for i in 0..3 {
                let mut hb = [0u8; 32];
                hb[0] = 0x10 + i as u8;
                let hash = H256::from(hb);
                let header = BlockHeader {
                    height: i as u32,
                    timestamp: i as u64,
                    parent_hash: parent,
                };
                db.set_block_header(hash, header);
                // `validate_block_above` also checks events presence
                // for every Eth block on the advance walk — set an
                // empty vector so the gate passes.
                db.set_block_events(hash, &[]);
                db.mutate_block_meta(hash, |_| {});
                hashes.push((hash, header));
                parent = hash;
            }
            hashes
        };
        let head = ethexe_common::SimpleBlockData {
            hash: chain_hashes[2].0,
            header: chain_hashes[2].1,
        };
        let (ext, _rx) = make_externalities(db.clone());
        set_head(&ext, head).await;

        let payload = Operations::new(vec![
            Operation::AdvanceTillEthereumBlock {
                block_hash: chain_hashes[1].0,
            },
            Operation::ProgressTasks,
            Operation::ProcessQueuesV3 { gas_allowance: 0 },
        ]);
        assert!(
            ext.validate_operations(H256::zero(), payload)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn build_emits_decryption_keys_without_other_proposable_content() {
        use ethexe_common::{SignedMessage, injected::InjectedTransaction};
        use gprimitives::ActorId;

        let db = Database::memory();
        let (mut ext, mut rx) = make_externalities(db);
        let (tdec_ctx, tdec_store, dkg_public_key) = single_validator_tdec_setup();
        ext.tdec_ctx = Some(tdec_ctx);
        ext.tdec_store = tdec_store;

        let injected = InjectedTransaction {
            destination: ActorId::from([1; 32]),
            payload: vec![1, 2, 3].try_into().unwrap(),
            value: 0,
            reference_block: H256::zero(),
            salt: vec![7; 32].try_into().unwrap(),
        };
        let mut rng = gear_tdec::rand_utils::test_rng();
        let shielded = injected.shield(&dkg_public_key, &mut rng).unwrap();
        let signed_shielded =
            SignedMessage::create(ethexe_common::PrivateKey::random(), shielded).unwrap();
        let shielded_hash = signed_shielded.data().to_hash();

        let parent_payload = Operations::new(vec![
            Operation::Shielded(signed_shielded),
            Operation::ProgressTasks,
            Operation::ProcessQueuesV3 { gas_allowance: 0 },
        ]);
        let parent = Block::new(H256::zero(), 1, to_payload(parent_payload.encode()));
        let parent_hash = parent.hash();
        ext.process_mb_proposal(parent_hash, parent).await.unwrap();
        let event = rx.recv().await.expect("block event").expect("ok");
        assert!(matches!(event, MalachiteEvent::BlockProposal { .. }));
        assert!(ext.extend_vote(parent_hash).await.unwrap().is_some());

        let operations = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            ext.build_operations(parent_hash),
        )
        .await
        .expect("decryption keys alone must be enough to build a block")
        .unwrap();

        let mut iter = operations.iter();
        let Some(Operation::DecryptionKeys(keys)) = iter.next() else {
            panic!("first operation must carry decryption keys");
        };
        assert!(keys.contains_key(&shielded_hash));
        assert!(matches!(iter.next(), Some(Operation::ProgressTasks)));
        assert!(matches!(
            iter.next(),
            Some(Operation::ProcessQueuesV3 { gas_allowance }) if *gas_allowance == ext.cfg.gas_allowance
        ));
        assert!(iter.next().is_none());
    }

    /// Stub mempool that records every `forget` argument so the test
    /// can assert which txs reached the mempool eviction path.
    #[derive(Default)]
    struct ForgetTracker {
        seen: tokio::sync::Mutex<Vec<TransactionHash>>,
    }

    #[async_trait::async_trait]
    impl Mempool for ForgetTracker {
        async fn insert(&self, _tx: Transaction) -> crate::mempool::TxInsertionStatus {
            crate::mempool::TxInsertionStatus::Inserted
        }

        async fn set_chain_head(&self, _head: SimpleBlockData) -> Vec<PurgedTransaction> {
            Vec::new()
        }

        async fn fetch(&self, _head: SimpleBlockData) -> Vec<Transaction> {
            Vec::new()
        }
        async fn forget(&self, committed: &[TransactionRef<'_>]) {
            self.seen
                .lock()
                .await
                .extend(committed.iter().map(TransactionRef::hash));
        }
        async fn wait_for_new_tx(&self) {
            std::future::pending().await
        }
    }

    /// `process_mb_finalized` must hand exactly the
    /// [`Operation::Injected`] subset of the committed block to
    /// [`Mempool::forget`] (and nothing else — service txs like
    /// `ProcessQueuesV3` stay out of the mempool round trip).
    #[tokio::test]
    async fn finalize_forgets_injected_txs() {
        use ethexe_common::{
            PrivateKey, SignedMessage, db::OnChainStorageRW, injected::InjectedTransaction,
        };
        use gprimitives::ActorId;

        let db = Database::memory();
        // Set up a single chain block so the injected txs reference a
        // valid `reference_block` — even though the stub mempool's
        // `insert` is a no-op, the value still travels through the
        // committed block intact.
        let ref_hash = H256::repeat_byte(0x42);
        let header = BlockHeader {
            height: 1,
            timestamp: 0,
            parent_hash: H256::zero(),
        };
        db.set_block_header(ref_hash, header);

        let pk = PrivateKey::random();
        let mk_tx = |salt: u8| {
            SignedMessage::create(
                pk.clone(),
                InjectedTransaction {
                    destination: ActorId::zero(),
                    payload: vec![1, 2, 3].try_into().unwrap(),
                    value: 0,
                    reference_block: ref_hash,
                    salt: vec![salt; 32].try_into().unwrap(),
                },
            )
            .unwrap()
        };
        let tx_a = mk_tx(1);
        let tx_b = mk_tx(2);

        let tracker = Arc::new(ForgetTracker::default());
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let ext = EthexeExternalities {
            db: db.clone(),
            tdec_ctx: None,
            decryption_shares: Arc::new(DecryptionSharesStore::new()),
            tdec_store: TdecKeyStore::memory(),
            mempool: Some(Arc::clone(&tracker) as Arc<dyn Mempool>),
            chain_head: make_chain_head(),
            event_tx,
            pending_events: RwLock::new(VecDeque::new()),
            cfg: ExternalitiesConfig {
                gas_allowance: 1_000_000,
                canonical_quarantine: 0,
                post_quarantine_delay: 0,
            },
        };

        let payload = Operations::new(vec![
            // service tx — must NOT show up in `forget`
            Operation::ProgressTasks,
            // user tx #1 — must show up
            Operation::Injected(tx_a.clone()),
            // service tx — must NOT
            Operation::ProcessQueuesV3 { gas_allowance: 0 },
            // user tx #2 — must show up
            Operation::Injected(tx_b.clone()),
        ]);
        let block = Block::new(H256::zero(), 1, to_payload(payload.encode()));
        let mb_hash = block.hash();
        ext.process_mb_proposal(mb_hash, block).await.unwrap();
        // Drain the BlockProposal event the save emits.
        let _ = event_rx.recv().await;
        ext.process_mb_finalized(
            mb_hash,
            ethexe_malachite_core::CommitCertificate {
                height: 1,
                block_hash: mb_hash,
                signatures: vec![],
            },
        )
        .await
        .unwrap();

        let seen_hashes = tracker.seen.lock().await.clone();
        assert_eq!(
            seen_hashes.len(),
            2,
            "exactly two injected txs should be forgotten"
        );
        assert!(seen_hashes.contains(&TransactionHash::Left(tx_a.data().to_hash())));
        assert!(seen_hashes.contains(&TransactionHash::Left(tx_b.data().to_hash())));
    }

    // ------------------------------------------------------------------
    // Integration tests for build_block_above / validate_block_above
    // size + touched-programs caps. Adapted from master's
    // `tx_pool::tests::*` and `announces::tests::*`.
    // ------------------------------------------------------------------

    /// Build a full `EthexeExternalities` wired to a real
    /// `InjectedTxMempool` so we can exercise the producer-side filter
    /// + caps end-to-end.
    fn make_externalities_with_pool(
        db: Database,
        mempool: Arc<crate::InjectedTxMempool>,
    ) -> (
        EthexeExternalities,
        mpsc::UnboundedReceiver<Result<MalachiteEvent>>,
    ) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let ext = EthexeExternalities {
            db,
            tdec_ctx: None,
            decryption_shares: Arc::new(DecryptionSharesStore::new()),
            tdec_store: TdecKeyStore::memory(),
            mempool: Some(mempool as Arc<dyn Mempool>),
            chain_head: make_chain_head(),
            event_tx,
            pending_events: RwLock::new(VecDeque::new()),
            cfg: ExternalitiesConfig {
                gas_allowance: 1_000_000,
                canonical_quarantine: 0,
                post_quarantine_delay: 0,
            },
        };
        (ext, event_rx)
    }

    /// Adapted from master's `setup_announce`: creates a fresh
    /// computed MB on top of `parent_mb` whose program-states map has
    /// one entry per `destinations`, each pointing at an initialised
    /// program with sufficient executable balance.
    fn setup_mb_with_destinations(
        db: &Database,
        parent_mb: H256,
        destinations: &[gprimitives::ActorId],
    ) -> H256 {
        use crate::tx_validity::MIN_EXECUTABLE_BALANCE_FOR_INJECTED_MESSAGES;
        use ethexe_common::{
            MaybeHashOf, StateHashWithQueueSize,
            db::{CompactMb, MbStorageRW},
        };
        use ethexe_runtime_common::state::{
            ActiveProgram, MessageQueueHashWithSize, Program, ProgramState, Storage,
        };

        let operations_hash = db.set_operations(Operations::new(vec![]));
        let mb_hash = H256::random();
        db.set_mb_compact_block(
            mb_hash,
            CompactMb {
                parent: parent_mb,
                height: u64::MAX / 2,
                operations_hash,
            },
        );

        let state = ProgramState {
            program: Program::Active(ActiveProgram {
                allocations_hash: MaybeHashOf::empty(),
                pages_hash: MaybeHashOf::empty(),
                memory_infix: ethexe_common::gear_core::program::MemoryInfix::new(0),
                initialized: true,
            }),
            canonical_queue: MessageQueueHashWithSize {
                hash: MaybeHashOf::empty(),
                cached_queue_size: 0,
            },
            injected_queue: MessageQueueHashWithSize {
                hash: MaybeHashOf::empty(),
                cached_queue_size: 0,
            },
            waitlist_hash: MaybeHashOf::empty(),
            stash_hash: MaybeHashOf::empty(),
            mailbox_hash: MaybeHashOf::empty(),
            balance: 0,
            executable_balance: MIN_EXECUTABLE_BALANCE_FOR_INJECTED_MESSAGES * 100,
        };
        let state_hash = db.write_program_state(state);
        let mut program_states = ethexe_common::ProgramStates::default();
        for dest in destinations {
            program_states.insert(
                *dest,
                StateHashWithQueueSize {
                    hash: state_hash,
                    canonical_queue_size: 0,
                    injected_queue_size: 0,
                },
            );
        }
        db.set_mb_program_states(mb_hash, program_states);
        db.mutate_mb_meta(mb_hash, |meta| meta.computed = true);
        mb_hash
    }

    fn signed_injected_tx(
        pk: &ethexe_common::PrivateKey,
        destination: gprimitives::ActorId,
        reference_block: H256,
        salt: u8,
    ) -> SignedInjectedTransaction {
        ethexe_common::SignedMessage::create(
            pk.clone(),
            ethexe_common::injected::InjectedTransaction {
                destination,
                payload: vec![1, 2, 3].try_into().unwrap(),
                value: 0,
                reference_block,
                salt: vec![salt; 32].try_into().unwrap(),
            },
        )
        .unwrap()
    }

    /// Port of master's `tx_pool::tests::test_select_for_announce`
    /// adapted to the MB world. A valid tx must surface in the
    /// produced MB; a non-zero-value tx is rejected at insert and
    /// never seen by `build_block_above`.
    #[tokio::test]
    async fn build_emits_valid_tx_and_drops_non_zero_value() {
        use ethexe_common::{
            injected::InjectedTransaction,
            mock::{BlockChain, Mock},
        };
        use gprimitives::ActorId;

        let db = Database::memory();
        let chain = BlockChain::mock(10u32).setup(&db);
        let dest = ActorId::from([1; 32]);
        let head = chain.blocks[10].to_simple();

        // Parent MB whose program_states snapshot includes `dest`.
        let parent_mb = setup_mb_with_destinations(&db, chain.mb_hash_at(9), &[dest]);

        let mempool = Arc::new(crate::InjectedTxMempool::new(db.clone()));
        // Drive validity-window GC.
        let _ = mempool.set_chain_head(head).await;

        let pk = ethexe_common::PrivateKey::random();
        let valid = signed_injected_tx(&pk, dest, chain.blocks[9].hash, 0);
        let value_tx = ethexe_common::SignedMessage::create(
            pk.clone(),
            InjectedTransaction {
                destination: dest,
                payload: vec![].try_into().unwrap(),
                value: 100,
                reference_block: chain.blocks[9].hash,
                salt: vec![1; 32].try_into().unwrap(),
            },
        )
        .unwrap();

        mempool.insert(valid.clone().into()).await;
        assert_eq!(
            mempool.insert(value_tx.clone().into()).await,
            crate::mempool::TxInsertionStatus::NonZeroValue,
        );
        assert_eq!(mempool.len().await, 1);

        let (ext, _rx) = make_externalities_with_pool(db, mempool);
        set_head(&ext, head).await;

        let payload = ext.build_operations(parent_mb).await.unwrap();
        let injected: Vec<_> = payload
            .iter()
            .filter_map(|tx| match tx {
                Operation::Injected(t) => Some(t.data().to_hash()),
                _ => None,
            })
            .collect();
        assert_eq!(injected, vec![valid.data().to_hash()]);
    }

    /// Port of master's `tx_pool::tests::max_touched_programs`. The
    /// pool holds 50 txs to 50 distinct destinations; the parent MB's
    /// `program_states` contains MAX+1 known programs, of which the
    /// first MAX/2 + 1 are already "touched" by EB events on the
    /// advance block. The producer can add at most `MAX - initial`
    /// injected txs before hitting the cap.
    #[tokio::test]
    async fn build_caps_touched_programs() {
        use ethexe_common::{
            MAX_TOUCHED_PROGRAMS_PER_MB,
            events::{BlockEvent, MirrorEvent, mirror::MessageQueueingRequestedEvent},
            mock::{BlockChain, Mock},
        };
        use gprimitives::{ActorId, MessageId};

        let db = Database::memory();
        // Seed events on block index 10: half-plus-one programs touched.
        let n_touched = (MAX_TOUCHED_PROGRAMS_PER_MB / 2 + 1) as u64;
        let mut chain = BlockChain::mock(10u32);
        chain.blocks[10].as_synced_mut().events = (0..n_touched)
            .map(|i| BlockEvent::Mirror {
                actor_id: ActorId::from(i),
                event: MirrorEvent::MessageQueueingRequested(MessageQueueingRequestedEvent {
                    id: MessageId::zero(),
                    source: ActorId::zero(),
                    payload: vec![],
                    value: 0,
                    call_reply: false,
                }),
            })
            .collect();
        let chain = chain.setup(&db);

        // All MAX+1 destinations exist in the latest computed snapshot.
        let n_destinations = (MAX_TOUCHED_PROGRAMS_PER_MB + 1) as u64;
        let destinations: Vec<ActorId> = (0..n_destinations).map(ActorId::from).collect();
        let parent_mb = setup_mb_with_destinations(&db, chain.mb_hash_at(9), &destinations);
        // eb_touched_programs needs latest_computed_mb_hash to find
        // known programs. Point it at the parent MB.
        db.globals_mutate(|g| g.latest_computed_mb_hash = parent_mb);

        let head = chain.blocks[10].to_simple();
        let mempool = Arc::new(crate::InjectedTxMempool::new(db.clone()));
        let _ = mempool.set_chain_head(head).await;
        let pk = ethexe_common::PrivateKey::random();
        // Push 50 txs targeting the upper half of destinations (the ones
        // NOT pre-touched by EB events).
        let push_start = MAX_TOUCHED_PROGRAMS_PER_MB / 2 + 1;
        let push_end = MAX_TOUCHED_PROGRAMS_PER_MB + 1;
        for i in push_start..push_end {
            mempool
                .insert(
                    signed_injected_tx(&pk, ActorId::from(i as u64), chain.blocks[9].hash, i as u8)
                        .into(),
                )
                .await;
        }

        let (ext, _rx) = make_externalities_with_pool(db.clone(), mempool);
        set_head(&ext, head).await;
        // Force AdvanceTillEthereumBlock so eb_touched_programs walks events.
        // The producer reads chain_head_notify to pick its advance candidate;
        // since canonical_quarantine = 0, head's parent (block 9) is a valid
        // advance.
        let payload = ext.build_operations(parent_mb).await.unwrap();
        let advance_present = payload
            .iter()
            .any(|tx| matches!(tx, Operation::AdvanceTillEthereumBlock { .. }));
        assert!(
            advance_present,
            "advance must be present for the EB-events touched seed to apply"
        );
        let injected_count = payload
            .iter()
            .filter(|tx| matches!(tx, Operation::Injected(_)))
            .count();
        // Master's expectation: producer can add at most
        // `MAX - already_touched` injected destinations.
        let expected = (MAX_TOUCHED_PROGRAMS_PER_MB as usize).saturating_sub(n_touched as usize);
        assert_eq!(
            injected_count, expected,
            "expected {expected} injected txs (MAX - initial_touched), got {injected_count}",
        );
    }

    /// Port of master's `announces::tests::reject_announce_with_too_many_touched_programs`.
    /// A participant must reject an MB whose injected destinations
    /// push the touched-programs total over the cap.
    #[tokio::test]
    async fn validate_rejects_mb_with_too_many_touched_programs() {
        use ethexe_common::{
            MAX_TOUCHED_PROGRAMS_PER_MB,
            events::{BlockEvent, MirrorEvent, mirror::MessageQueueingRequestedEvent},
            mock::{BlockChain, Mock},
        };
        use gprimitives::{ActorId, MessageId};

        let db = Database::memory();
        let n_touched = (MAX_TOUCHED_PROGRAMS_PER_MB / 2 + 1) as u64;
        let mut chain = BlockChain::mock(10u32);
        chain.blocks[10].as_synced_mut().events = (0..n_touched)
            .map(|i| BlockEvent::Mirror {
                actor_id: ActorId::from(i),
                event: MirrorEvent::MessageQueueingRequested(MessageQueueingRequestedEvent {
                    id: MessageId::zero(),
                    source: ActorId::zero(),
                    payload: vec![],
                    value: 0,
                    call_reply: false,
                }),
            })
            .collect();
        let chain = chain.setup(&db);

        let n_destinations = (MAX_TOUCHED_PROGRAMS_PER_MB + 1) as u64;
        let destinations: Vec<ActorId> = (0..n_destinations).map(ActorId::from).collect();
        let parent_mb = setup_mb_with_destinations(&db, chain.mb_hash_at(9), &destinations);
        db.globals_mutate(|g| g.latest_computed_mb_hash = parent_mb);

        let head = chain.blocks[10].to_simple();
        let (ext, _rx) = make_externalities(db.clone());
        set_head(&ext, head).await;

        // Craft an MB payload that adds N/2 fresh destinations on top
        // of the N/2+1 already touched by EB events → total > limit.
        // Advance to block 10 so `eb_touched_programs` walks the
        // events block we just seeded.
        let advance_block = chain.blocks[10].hash;
        let pk = ethexe_common::PrivateKey::random();
        let extra_destinations = (MAX_TOUCHED_PROGRAMS_PER_MB / 2 + 1
            ..MAX_TOUCHED_PROGRAMS_PER_MB + 1)
            .map(|i| ActorId::from(i as u64));
        let mut operations = vec![Operation::AdvanceTillEthereumBlock {
            block_hash: advance_block,
        }];
        for (i, dest) in extra_destinations.enumerate() {
            operations.push(Operation::Injected(signed_injected_tx(
                &pk,
                dest,
                chain.blocks[9].hash,
                i as u8,
            )));
        }
        // Full shape — the shape walk must not be the reason for rejection.
        operations.push(Operation::ProgressTasks);
        operations.push(Operation::ProcessQueuesV3 { gas_allowance: 0 });
        let payload = Operations::new(operations);
        assert!(
            !ext.validate_operations(parent_mb, payload).await.unwrap(),
            "MB must be rejected when touched destinations + EB-touched > cap"
        );
    }

    /// Port of master's idea: a fully-sized batch of injected txs
    /// must trip the per-MB size cap. We feed the pool enough txs that
    /// their cumulative encoded size exceeds
    /// `MAX_INJECTED_TRANSACTIONS_SIZE_PER_MB`; the producer must
    /// emit only as many as fit.
    #[tokio::test]
    async fn build_caps_total_encoded_size() {
        use ethexe_common::{
            injected::{
                InjectedTransaction, MAX_INJECTED_TRANSACTIONS_SIZE_PER_MB,
                MAX_INJECTED_TX_PAYLOAD_SIZE,
            },
            mock::{BlockChain, Mock},
        };
        use gprimitives::ActorId;
        use parity_scale_codec::Encode;

        let db = Database::memory();
        let chain = BlockChain::mock(2u32).setup(&db);
        let head = chain.blocks[2].to_simple();
        // Many destinations so the touched-programs cap can't be the
        // limiting factor (one destination per tx).
        let dests: Vec<ActorId> = (0..16u64).map(ActorId::from).collect();
        let parent_mb = setup_mb_with_destinations(&db, chain.mb_hash_at(1), &dests);
        db.globals_mutate(|g| g.latest_computed_mb_hash = parent_mb);

        let mempool = Arc::new(crate::InjectedTxMempool::new(db.clone()));
        let _ = mempool.set_chain_head(head).await;
        let pk = ethexe_common::PrivateKey::random();
        // Each tx carries the maximum-size payload; the pool is loaded
        // with enough of them that two fit but three don't.
        for (i, dest) in dests.iter().enumerate().take(3) {
            let tx = ethexe_common::SignedMessage::create(
                pk.clone(),
                InjectedTransaction {
                    destination: *dest,
                    payload: vec![0u8; MAX_INJECTED_TX_PAYLOAD_SIZE / 2]
                        .try_into()
                        .unwrap(),
                    value: 0,
                    reference_block: chain.blocks[1].hash,
                    salt: vec![i as u8; 32].try_into().unwrap(),
                },
            )
            .unwrap();
            mempool.insert(tx.into()).await;
        }
        assert_eq!(mempool.len().await, 3);

        let (ext, _rx) = make_externalities_with_pool(db.clone(), mempool);
        set_head(&ext, head).await;

        let payload = ext.build_operations(parent_mb).await.unwrap();
        let injected: Vec<_> = payload
            .iter()
            .filter_map(|tx| match tx {
                Operation::Injected(t) => Some(t.encoded_size()),
                _ => None,
            })
            .collect();
        let total: usize = injected.iter().sum();
        assert!(
            total <= MAX_INJECTED_TRANSACTIONS_SIZE_PER_MB,
            "cumulative encoded size ({total}) exceeds per-MB cap ({MAX_INJECTED_TRANSACTIONS_SIZE_PER_MB})",
        );
        // With 3 half-payload txs each ~64 KiB, only the first two should
        // fit under the 127 KiB cap. (Each encoded tx adds ~64 KiB + envelope.)
        assert!(
            injected.len() < 3,
            "size cap must drop at least one tx, got {} retained",
            injected.len()
        );
    }

    // ------------------------------------------------------------------
    // Shape & ordering checks on `validate_block_above`.
    //
    // Every MB the producer emits has the strict shape
    //   [AdvanceTillEthereumBlock]?  Injected*  ProgressTasks  ProcessQueuesV3
    // with `ProcessQueuesV3.gas_allowance <= DEFAULT_GAS_ALLOWANCE`.
    // A malicious proposer must not be able to slip in a malformed MB
    // (oversized gas, missing bookend, out-of-order tx).
    // ------------------------------------------------------------------

    /// Helper: build a tiny chain with one block past quarantine and an
    /// `EthexeExternalities` whose chain_head points at it. Returns the
    /// ext + the advance block hash to use for `AdvanceTillEthereumBlock`.
    async fn chain_with_one_advance(
        db: Database,
    ) -> (
        EthexeExternalities,
        mpsc::UnboundedReceiver<Result<MalachiteEvent>>,
        H256,
    ) {
        let mut parent = H256::zero();
        let mut chain_hashes = Vec::new();
        for i in 0..3u8 {
            let mut hb = [0u8; 32];
            hb[0] = 0x10 + i;
            let hash = H256::from(hb);
            let header = BlockHeader {
                height: i as u32,
                timestamp: i as u64,
                parent_hash: parent,
            };
            db.set_block_header(hash, header);
            db.set_block_events(hash, &[]);
            db.mutate_block_meta(hash, |_| {});
            chain_hashes.push((hash, header));
            parent = hash;
        }
        let head = ethexe_common::SimpleBlockData {
            hash: chain_hashes[2].0,
            header: chain_hashes[2].1,
        };
        let advance_hash = chain_hashes[1].0;
        let (ext, rx) = make_externalities(db);
        set_head(&ext, head).await;
        (ext, rx, advance_hash)
    }

    /// REPRODUCES: a malicious proposer can set `gas_allowance = u64::MAX`
    /// in `ProcessQueuesV3.gas_allowance` and force every participant to attempt
    /// an unbounded queue drain. Validator must reject MBs whose
    /// `gas_allowance` exceeds the protocol cap
    /// (`MalachiteServiceConfig::DEFAULT_GAS_ALLOWANCE`).
    #[tokio::test]
    async fn validate_rejects_gas_allowance_above_default() {
        let db = Database::memory();
        let (ext, _rx, advance) = chain_with_one_advance(db).await;
        let payload = Operations::new(vec![
            Operation::AdvanceTillEthereumBlock {
                block_hash: advance,
            },
            Operation::ProgressTasks,
            Operation::ProcessQueuesV3 {
                gas_allowance: u64::MAX,
            },
        ]);
        assert!(
            !ext.validate_operations(H256::zero(), payload)
                .await
                .unwrap(),
            "MB with `gas_allowance > DEFAULT_GAS_ALLOWANCE` must be rejected"
        );
    }

    /// REPRODUCES: MB without a `ProgressTasks` tx between injected txs
    /// and `ProcessQueuesV3` violates the protocol shape — scheduled
    /// tasks would never be advanced.
    #[tokio::test]
    async fn validate_rejects_mb_missing_progress_tasks() {
        let db = Database::memory();
        let (ext, _rx, advance) = chain_with_one_advance(db).await;
        let payload = Operations::new(vec![
            Operation::AdvanceTillEthereumBlock {
                block_hash: advance,
            },
            // No ProgressTasks here — straight to ProcessQueuesV3.
            Operation::ProcessQueuesV3 { gas_allowance: 0 },
        ]);
        assert!(
            !ext.validate_operations(H256::zero(), payload)
                .await
                .unwrap(),
            "MB missing `ProgressTasks` bookend must be rejected"
        );
    }

    /// REPRODUCES: MB without a final `ProcessQueuesV3` tx never drains
    /// the message queues for this MB — compute pipeline would stall
    /// on the next MB.
    #[tokio::test]
    async fn validate_rejects_mb_missing_process_queues() {
        let db = Database::memory();
        let (ext, _rx, advance) = chain_with_one_advance(db).await;
        let payload = Operations::new(vec![
            Operation::AdvanceTillEthereumBlock {
                block_hash: advance,
            },
            Operation::ProgressTasks,
            // No ProcessQueuesV3 here.
        ]);
        assert!(
            !ext.validate_operations(H256::zero(), payload)
                .await
                .unwrap(),
            "MB missing `ProcessQueuesV3` bookend must be rejected"
        );
    }

    /// REPRODUCES: `AdvanceTillEthereumBlock` must be the very first tx
    /// in the MB. Otherwise EB events are not the first action of the
    /// MB and compute pipeline runs queues against stale state.
    #[tokio::test]
    async fn validate_rejects_advance_not_first() {
        use ethexe_common::{
            PrivateKey, SignedMessage,
            injected::InjectedTransaction,
            mock::{BlockChain, Mock},
        };

        let db = Database::memory();
        let chain = BlockChain::mock(2u32).setup(&db);
        let head = chain.blocks[2].to_simple();
        let dest = gprimitives::ActorId::from([1; 32]);
        let parent_mb = setup_mb_with_destinations(&db, chain.mb_hash_at(1), &[dest]);
        db.globals_mutate(|g| g.latest_computed_mb_hash = parent_mb);

        let (ext, _rx) = make_externalities(db.clone());
        set_head(&ext, head).await;

        let pk = PrivateKey::random();
        let tx = SignedMessage::create(
            pk.clone(),
            InjectedTransaction {
                destination: dest,
                payload: vec![].try_into().unwrap(),
                value: 0,
                reference_block: chain.blocks[1].hash,
                salt: vec![7; 32].try_into().unwrap(),
            },
        )
        .unwrap();
        let payload = Operations::new(vec![
            // Order swapped: Injected before Advance.
            Operation::Injected(tx),
            Operation::AdvanceTillEthereumBlock {
                block_hash: chain.blocks[2].hash,
            },
            Operation::ProgressTasks,
            Operation::ProcessQueuesV3 { gas_allowance: 0 },
        ]);
        assert!(
            !ext.validate_operations(parent_mb, payload).await.unwrap(),
            "MB where Advance is not the first tx must be rejected"
        );
    }

    /// REPRODUCES: `ProcessQueuesV3` must be the very last tx in the MB.
    /// Otherwise later txs run *after* queues drain and the gas budget
    /// is wrong.
    #[tokio::test]
    async fn validate_rejects_process_queues_not_last() {
        let db = Database::memory();
        let (ext, _rx, advance) = chain_with_one_advance(db).await;
        let payload = Operations::new(vec![
            Operation::AdvanceTillEthereumBlock {
                block_hash: advance,
            },
            // Order swapped: ProcessQueuesV3 before ProgressTasks.
            Operation::ProcessQueuesV3 { gas_allowance: 0 },
            Operation::ProgressTasks,
        ]);
        assert!(
            !ext.validate_operations(H256::zero(), payload)
                .await
                .unwrap(),
            "MB where `ProcessQueuesV3` is not the last tx must be rejected"
        );
    }

    #[tokio::test]
    async fn validate_rejects_advance_that_regresses_last_advanced_eb() {
        let db = Database::memory();

        // Build a 5-block chain.
        let mut parent = H256::zero();
        let mut chain = Vec::new();
        for i in 0..5u8 {
            let mut hb = [0u8; 32];
            hb[0] = 0x10 + i;
            let hash = H256::from(hb);
            let header = BlockHeader {
                height: i as u32,
                timestamp: i as u64,
                parent_hash: parent,
            };
            db.set_block_header(hash, header);
            db.set_block_events(hash, &[]);
            db.mutate_block_meta(hash, |_| {});
            chain.push((hash, header));
            parent = hash;
        }
        let head = ethexe_common::SimpleBlockData {
            hash: chain[4].0,
            header: chain[4].1,
        };

        // Seed a parent MB whose `last_advanced_eb` points at chain[3]
        // (a relatively recent EB). The proposer's `advance` then points
        // at chain[1] (regressing). Both pass quarantine (depth = 1+ vs.
        // `canonical_quarantine = 0`), but chain[1] is a strict ancestor
        // of chain[3], so the descendant check would reject — and that
        // is exactly what we want validators to do.
        let parent_mb = H256::from([0xCD; 32]);
        let operations_hash = db.set_operations(Operations::new(vec![]));
        db.set_mb_compact_block(
            parent_mb,
            ethexe_common::db::CompactMb {
                parent: H256::zero(),
                height: 1,
                operations_hash,
            },
        );
        db.set_mb_program_states(parent_mb, ethexe_common::ProgramStates::default());
        db.mutate_mb_meta(parent_mb, |meta| {
            meta.computed = true;
            meta.last_advanced_eb = chain[3].0;
        });

        let (ext, _rx) = make_externalities(db.clone());
        set_head(&ext, head).await;

        // MB proposes Advance to chain[1] — strictly older than chain[3]
        // (parent's last_advanced_eb). `verify_passed` accepts (chain[1]
        // is a canonical ancestor of head); `is_strict_descendant_of`
        // would reject (chain[1] does NOT descend from chain[3]).
        let payload = Operations::new(vec![
            Operation::AdvanceTillEthereumBlock {
                block_hash: chain[1].0,
            },
            Operation::ProgressTasks,
            Operation::ProcessQueuesV3 { gas_allowance: 0 },
        ]);

        assert!(
            !ext.validate_operations(parent_mb, payload).await.unwrap(),
            "MB whose AdvanceTillEthereumBlock regresses parent.last_advanced_eb \
             must be rejected — currently passes because validate_block_above \
             skips the strict-descendant check the producer enforces",
        );
    }

    /// `validate_block_above` must return synchronously when the local
    /// view does not yet cover the proposer's `advance`. Previously it
    /// polled in a 2-second loop; now it abstains immediately so the
    /// engine can rotate to the next proposer. The 50 ms timeout below
    /// is *much* shorter than the old 2 s poll budget — if the function
    /// still waited, the timeout would fire.
    #[tokio::test]
    async fn validate_rejects_advance_when_chain_head_does_not_cover_it() {
        let db = Database::memory();

        // Build a small canonical chain `[c0, c1, c2]`.
        let mut parent = H256::zero();
        let mut chain = Vec::new();
        for i in 0..3u8 {
            let mut hb = [0u8; 32];
            hb[0] = 0x20 + i;
            let hash = H256::from(hb);
            let header = BlockHeader {
                height: i as u32,
                timestamp: i as u64,
                parent_hash: parent,
            };
            db.set_block_header(hash, header);
            db.set_block_events(hash, &[]);
            db.mutate_block_meta(hash, |_| {});
            chain.push((hash, header));
            parent = hash;
        }
        // Local chain_head = c1 (lagging behind the proposer's view).
        let head = ethexe_common::SimpleBlockData {
            hash: chain[1].0,
            header: chain[1].1,
        };

        // Proposer's `advance` points at a block our DB has no
        // canonical-ancestor record for from `head` — a fully
        // unrelated hash. `verify_passed` will return Err and
        // validation must reject in one shot.
        let stranger_advance = H256::from([0xEE; 32]);

        let (ext, _rx) = make_externalities(db.clone());
        set_head(&ext, head).await;

        let payload = Operations::new(vec![
            Operation::AdvanceTillEthereumBlock {
                block_hash: stranger_advance,
            },
            Operation::ProgressTasks,
            Operation::ProcessQueuesV3 { gas_allowance: 0 },
        ]);

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            ext.validate_operations(H256::zero(), payload),
        )
        .await
        .expect("validate_block_above must return synchronously, not wait on local sync");
        assert!(
            !result.unwrap(),
            "MB with an advance the local view doesn't cover must be rejected"
        );
    }

    /// Producer-side: `find_eb_candidate_for_advancing` picks the
    /// anchor at depth `canonical_quarantine + post_quarantine_delay`
    /// from the local chain head. With `(canonical_quarantine = 2,
    /// post_quarantine_delay = 3)` the candidate sits 5 blocks below
    /// head — exactly what we verify by walking parent links.
    #[tokio::test]
    async fn producer_picks_anchor_at_canonical_plus_post_delay() {
        use ethexe_common::db::OnChainStorageRO;

        let db = Database::memory();

        // Long chain — `canonical_quarantine + post_quarantine_delay`
        // must walk back 5 blocks, so we need at least 7 to leave
        // headroom and confirm the walk stops at the right depth.
        let mut parent = H256::zero();
        let mut chain = Vec::new();
        for i in 0..8u8 {
            let mut hb = [0u8; 32];
            hb[0] = 0x30 + i;
            let hash = H256::from(hb);
            let header = BlockHeader {
                height: i as u32,
                timestamp: i as u64,
                parent_hash: parent,
            };
            db.set_block_header(hash, header);
            db.set_block_events(hash, &[]);
            db.mutate_block_meta(hash, |_| {});
            chain.push((hash, header));
            parent = hash;
        }
        // `start_block_hash = chain[0]` keeps the fence at genesis so it
        // never trips for this walk.
        db.globals_mutate(|g| g.start_block_hash = chain[0].0);

        let head_idx = chain.len() - 1;
        let head = ethexe_common::SimpleBlockData {
            hash: chain[head_idx].0,
            header: chain[head_idx].1,
        };

        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let ext = EthexeExternalities {
            db: db.clone(),
            tdec_ctx: None,
            decryption_shares: Arc::new(DecryptionSharesStore::new()),
            tdec_store: TdecKeyStore::memory(),
            mempool: Some(Arc::new(EmptyMempool)),
            chain_head: make_chain_head(),
            event_tx,
            pending_events: RwLock::new(VecDeque::new()),
            cfg: ExternalitiesConfig {
                gas_allowance: 1_000_000,
                canonical_quarantine: 2,
                post_quarantine_delay: 3,
            },
        };
        set_head(&ext, head).await;

        let candidate = ext
            .find_eb_candidate_for_advancing(H256::zero())
            .await
            .unwrap()
            .expect("must surface a candidate — chain is deep enough");
        // Walk back `2 + 3 = 5` parents from head; that's the expected
        // anchor.
        let mut cursor = head.hash;
        for _ in 0..5 {
            let h = db
                .block_header(cursor)
                .expect("test chain headers are populated");
            cursor = h.parent_hash;
        }
        assert_eq!(
            candidate, cursor,
            "anchor must sit at depth `canonical_quarantine + post_quarantine_delay` from head",
        );
        let candidate_height = db.block_header(candidate).unwrap().height;
        assert_eq!(
            candidate_height,
            chain[head_idx].1.height - 5,
            "depth arithmetic mismatch — expected exactly 5 blocks below head",
        );
    }
}
