// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use sc_utils::notification::{NotificationSender, NotificationStream, TracingKeyStr};
use sp_runtime::traits::Block as BlockT;

use crate::justification::BeefyVersionedFinalityProof;

/// The sending half of the notifications channel(s) used to send
/// notifications about best BEEFY block from the gadget side.
pub type BeefyBestBlockSender<Block> = NotificationSender<<Block as BlockT>::Hash>;

/// The receiving half of a notifications channel used to receive
/// notifications about best BEEFY blocks determined on the gadget side.
pub type BeefyBestBlockStream<Block> =
    NotificationStream<<Block as BlockT>::Hash, BeefyBestBlockTracingKey>;

/// The sending half of the notifications channel(s) used to send notifications
/// about versioned finality proof generated at the end of a BEEFY round.
pub type BeefyVersionedFinalityProofSender<Block, AuthorityId> =
    NotificationSender<BeefyVersionedFinalityProof<Block, AuthorityId>>;

/// The receiving half of a notifications channel used to receive notifications
/// about versioned finality proof generated at the end of a BEEFY round.
pub type BeefyVersionedFinalityProofStream<Block, AuthorityId> = NotificationStream<
    BeefyVersionedFinalityProof<Block, AuthorityId>,
    BeefyVersionedFinalityProofTracingKey,
>;

/// Provides tracing key for BEEFY best block stream.
#[derive(Clone)]
pub struct BeefyBestBlockTracingKey;
impl TracingKeyStr for BeefyBestBlockTracingKey {
    const TRACING_KEY: &'static str = "mpsc_beefy_best_block_notification_stream";
}

/// Provides tracing key for BEEFY versioned finality proof stream.
#[derive(Clone)]
pub struct BeefyVersionedFinalityProofTracingKey;
impl TracingKeyStr for BeefyVersionedFinalityProofTracingKey {
    const TRACING_KEY: &'static str = "mpsc_beefy_versioned_finality_proof_notification_stream";
}
