// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! BEEFY gadget specific errors
//!
//! Used for BEEFY gadget internal error handling only

use sp_blockchain::Error as ClientError;
use std::fmt::Debug;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Backend: {0}")]
    Backend(String),
    #[error("Keystore error: {0}")]
    Keystore(String),
    #[error("Runtime api error: {0}")]
    RuntimeApi(sp_api::ApiError),
    #[error("Signature error: {0}")]
    Signature(String),
    #[error("Session uninitialized")]
    UninitSession,
    #[error("pallet-beefy was reset")]
    ConsensusReset,
    #[error("Block import stream terminated")]
    BlockImportStreamTerminated,
    #[error("Gossip Engine terminated")]
    GossipEngineTerminated,
    #[error("Finality proofs gossiping stream terminated")]
    FinalityProofGossipStreamTerminated,
    #[error("Finality stream terminated")]
    FinalityStreamTerminated,
    #[error("Votes gossiping stream terminated")]
    VotesGossipStreamTerminated,
}

impl From<ClientError> for Error {
    fn from(e: ClientError) -> Self {
        Self::Backend(e.to_string())
    }
}

#[cfg(test)]
impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Error::Backend(s1), Error::Backend(s2)) => s1 == s2,
            (Error::Keystore(s1), Error::Keystore(s2)) => s1 == s2,
            (Error::RuntimeApi(_), Error::RuntimeApi(_)) => true,
            (Error::Signature(s1), Error::Signature(s2)) => s1 == s2,
            (Error::UninitSession, Error::UninitSession) => true,
            (Error::ConsensusReset, Error::ConsensusReset) => true,
            _ => false,
        }
    }
}
