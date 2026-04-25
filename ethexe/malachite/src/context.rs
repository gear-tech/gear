// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

//! Custom [`Context`] for Malachite backed by **secp256k1** / ECDSA
//! signatures (the same curve ethexe uses everywhere else on-chain).
//!
//! We reuse [`malachitebft_test::Height`] — it's a thin `u64` newtype
//! with no crypto inside — and re-implement everything that touches
//! keys or addresses:
//!
//! - [`Address`] — a newtype over [`gsigner::schemes::secp256k1::Address`]
//!   (20 bytes, `keccak256(uncompressed_pubkey)[12..]`). The orphan
//!   rule forces a newtype because we need to
//!   `impl malachitebft_core_types::Address` for it.
//! - `SigningScheme = malachitebft_signing_ecdsa::K256` — upstream
//!   Ecdsa-over-`k256` scheme; its `PrivateKey`/`PublicKey`/`Signature`
//!   are the associated crypto types.
//! - [`EthexeSigner`] — our [`SigningProvider<EthexeContext>`] backed
//!   by the same 32-byte secret the node's validator identity is
//!   built from. No separate Malachite key, no node_key.json.

use core::slice;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use gprimitives::H256;
use parity_scale_codec::Encode;
use serde::{Deserialize, Serialize};
use sha3::{Digest as _, Keccak256};

use malachitebft_core_types::{
    Context, LinearTimeouts, NilOrVal, Round, SignedExtension, SignedMessage, SignedProposal,
    SignedVote, VoteType, VotingPower,
};
// Bring trait methods (`.count()`, `.id()`) into scope so they take
// precedence over any unrelated inherent methods with the same name.
use malachitebft_core_types::ValidatorSet as _ValidatorSetTrait;
use malachitebft_core_types::Value as _ValueTrait;
use malachitebft_signing::{Error as SigningError, SigningProvider, VerificationResult};
use malachitebft_signing_ecdsa::{K256, K256Config};

pub use malachitebft_test::Height;

use ethexe_common::mb::SequencerBlock;

pub type PublicKey = malachitebft_signing_ecdsa::PublicKey<K256Config>;
pub type PrivateKey = malachitebft_signing_ecdsa::PrivateKey<K256Config>;
pub type Signature = malachitebft_signing_ecdsa::Signature<K256Config>;

// ---------------------------------------------------------------------------
// Address — newtype over gsigner::secp256k1::Address (orphan rule)
// ---------------------------------------------------------------------------

/// 20-byte Ethereum-style address derived from a secp256k1 public key
/// the same way the rest of ethexe derives it:
/// `keccak256(uncompressed_pubkey[1..])[12..]`.
///
/// Wraps [`gsigner::schemes::secp256k1::Address`]. We cannot impl the
/// foreign [`malachitebft_core_types::Address`] marker trait for the
/// foreign gsigner type directly, hence the newtype.
#[derive(
    Copy,
    Clone,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    derive_more::Display,
    derive_more::Debug,
    Serialize,
    Deserialize,
)]
#[serde(transparent)]
#[display("{_0}")]
#[debug("{_0:?}")]
pub struct Address(pub gsigner::schemes::secp256k1::Address);

impl Address {
    /// Derive the address from a secp256k1 public key.
    pub fn from_public_key(public_key: &PublicKey) -> Self {
        // SEC1 uncompressed point: 0x04 || x(32) || y(32) = 65 bytes.
        let encoded = public_key.inner().to_encoded_point(false);
        let bytes = encoded.as_bytes();
        debug_assert_eq!(bytes.len(), 65);
        let mut hasher = Keccak256::new();
        hasher.update(&bytes[1..]);
        let hash = hasher.finalize();
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&hash[12..]);
        Self(gsigner::schemes::secp256k1::Address(addr))
    }

    pub fn into_inner(self) -> gsigner::schemes::secp256k1::Address {
        self.0
    }
}

impl From<gsigner::schemes::secp256k1::Address> for Address {
    fn from(inner: gsigner::schemes::secp256k1::Address) -> Self {
        Self(inner)
    }
}

impl From<Address> for gsigner::schemes::secp256k1::Address {
    fn from(outer: Address) -> Self {
        outer.0
    }
}

impl malachitebft_core_types::Address for Address {}

// ---------------------------------------------------------------------------
// Value + ValueId
// ---------------------------------------------------------------------------

/// Our [`SequencerBlock`] lifted into a `Value` for Malachite. Wraps
/// the block plus any opaque extensions propagated between heights
/// (unused for now, kept to preserve the engine's expected shape).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Value {
    pub block: SequencerBlock,
    #[serde(with = "serde_bytes")]
    pub extensions: Vec<u8>,
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Value {
    /// Order values by their `id()` (keccak-256 hash). The derivation
    /// is deterministic and content-addressed so this gives a stable
    /// total order for BFT equivocation-detection purposes.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id().0.cmp(&other.id().0)
    }
}

impl Value {
    pub fn new(block: SequencerBlock) -> Self {
        Self {
            block,
            extensions: Vec::new(),
        }
    }

    pub fn size_bytes(&self) -> usize {
        self.block.encode().len() + self.extensions.len()
    }
}

impl malachitebft_core_types::Value for Value {
    type Id = ValueId;

    fn id(&self) -> Self::Id {
        ValueId(self.block.hash())
    }
}

/// Content-addressed identifier for a [`Value`] — keccak-256 of the
/// SCALE-encoded [`SequencerBlock`], widened to 32 bytes (the block's
/// hash is already 32 bytes by construction).
#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct ValueId(pub H256);

impl ValueId {
    pub const fn new(h: H256) -> Self {
        Self(h)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_fixed_bytes()
    }
}

impl std::fmt::Display for ValueId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Validator + ValidatorSet
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Validator {
    pub address: Address,
    pub public_key: PublicKey,
    pub voting_power: VotingPower,
}

impl Validator {
    pub fn new(public_key: PublicKey, voting_power: VotingPower) -> Self {
        Self {
            address: Address::from_public_key(&public_key),
            public_key,
            voting_power,
        }
    }

    /// Construct with an explicit address — useful when the set comes
    /// from a genesis file that carries addresses and pubkeys
    /// independently (consistency is checked at load time).
    pub fn with_address(
        address: Address,
        public_key: PublicKey,
        voting_power: VotingPower,
    ) -> Self {
        Self {
            address,
            public_key,
            voting_power,
        }
    }
}

impl PartialOrd for Validator {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Validator {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.address.cmp(&other.address)
    }
}

impl malachitebft_core_types::Validator<EthexeContext> for Validator {
    fn address(&self) -> &Address {
        &self.address
    }

    fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    fn voting_power(&self) -> VotingPower {
        self.voting_power
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatorSet {
    pub validators: Arc<Vec<Validator>>,
}

impl ValidatorSet {
    pub fn new(validators: impl IntoIterator<Item = Validator>) -> Self {
        let mut v: Vec<_> = validators.into_iter().collect();
        assert!(!v.is_empty(), "validator set must be non-empty");
        // Stable deterministic ordering by address so all nodes see
        // the same "index → validator" mapping regardless of load
        // order.
        v.sort();
        Self {
            validators: Arc::new(v),
        }
    }

    pub fn len(&self) -> usize {
        self.validators.len()
    }

    pub fn is_empty(&self) -> bool {
        self.validators.is_empty()
    }

    pub fn iter(&self) -> slice::Iter<'_, Validator> {
        self.validators.iter()
    }

    pub fn total_voting_power(&self) -> VotingPower {
        self.validators.iter().map(|v| v.voting_power).sum()
    }

    pub fn get_by_index(&self, index: usize) -> Option<&Validator> {
        self.validators.get(index)
    }

    pub fn get_by_address(&self, address: &Address) -> Option<&Validator> {
        self.validators.iter().find(|v| &v.address == address)
    }

    pub fn get_by_public_key(&self, public_key: &PublicKey) -> Option<&Validator> {
        self.validators.iter().find(|v| &v.public_key == public_key)
    }

    pub fn get_keys(&self) -> Vec<PublicKey> {
        self.validators.iter().map(|v| v.public_key.clone()).collect()
    }
}

impl malachitebft_core_types::ValidatorSet<EthexeContext> for ValidatorSet {
    fn count(&self) -> usize {
        self.validators.len()
    }

    fn total_voting_power(&self) -> VotingPower {
        self.total_voting_power()
    }

    fn get_by_address(&self, address: &Address) -> Option<&Validator> {
        self.get_by_address(address)
    }

    fn get_by_index(&self, index: usize) -> Option<&Validator> {
        self.get_by_index(index)
    }
}

// ---------------------------------------------------------------------------
// Genesis wrapper
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Genesis {
    pub validator_set: ValidatorSet,
}

// ---------------------------------------------------------------------------
// Vote
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vote {
    pub typ: VoteType,
    pub height: Height,
    pub round: Round,
    pub value: NilOrVal<ValueId>,
    pub validator_address: Address,
    /// Vote extensions are not yet used — kept as `None` and excluded
    /// from serde so we don't have to wire `SignedMessage`'s codec.
    #[serde(skip)]
    pub extension: Option<SignedExtension<EthexeContext>>,
}

impl Vote {
    pub fn new_prevote(
        height: Height,
        round: Round,
        value: NilOrVal<ValueId>,
        validator_address: Address,
    ) -> Self {
        Self {
            typ: VoteType::Prevote,
            height,
            round,
            value,
            validator_address,
            extension: None,
        }
    }

    pub fn new_precommit(
        height: Height,
        round: Round,
        value: NilOrVal<ValueId>,
        validator_address: Address,
    ) -> Self {
        Self {
            typ: VoteType::Precommit,
            height,
            round,
            value,
            validator_address,
            extension: None,
        }
    }

    pub fn to_sign_bytes(&self) -> Bytes {
        serde_json::to_vec(self)
            .expect("Vote is serde-serializable")
            .into()
    }

    #[allow(dead_code)]
    pub fn from_sign_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

impl malachitebft_core_types::Vote<EthexeContext> for Vote {
    fn height(&self) -> Height {
        self.height
    }

    fn round(&self) -> Round {
        self.round
    }

    fn value(&self) -> &NilOrVal<ValueId> {
        &self.value
    }

    fn take_value(self) -> NilOrVal<ValueId> {
        self.value
    }

    fn vote_type(&self) -> VoteType {
        self.typ
    }

    fn validator_address(&self) -> &Address {
        &self.validator_address
    }

    fn extension(&self) -> Option<&SignedExtension<EthexeContext>> {
        self.extension.as_ref()
    }

    fn take_extension(&mut self) -> Option<SignedExtension<EthexeContext>> {
        self.extension.take()
    }

    fn extend(self, extension: SignedExtension<EthexeContext>) -> Self {
        Self {
            extension: Some(extension),
            ..self
        }
    }
}

// ---------------------------------------------------------------------------
// Proposal
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub height: Height,
    pub round: Round,
    pub value: Value,
    pub pol_round: Round,
    pub proposer: Address,
}

impl Proposal {
    pub fn new(
        height: Height,
        round: Round,
        value: Value,
        pol_round: Round,
        proposer: Address,
    ) -> Self {
        Self {
            height,
            round,
            value,
            pol_round,
            proposer,
        }
    }

    pub fn to_sign_bytes(&self) -> Bytes {
        serde_json::to_vec(self)
            .expect("Proposal is serde-serializable")
            .into()
    }

    #[allow(dead_code)]
    pub fn from_sign_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

impl malachitebft_core_types::Proposal<EthexeContext> for Proposal {
    fn height(&self) -> Height {
        self.height
    }

    fn round(&self) -> Round {
        self.round
    }

    fn value(&self) -> &Value {
        &self.value
    }

    fn take_value(self) -> Value {
        self.value
    }

    fn pol_round(&self) -> Round {
        self.pol_round
    }

    fn validator_address(&self) -> &Address {
        &self.proposer
    }
}

// ---------------------------------------------------------------------------
// ProposalPart (streamed)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalPart {
    Init(ProposalInit),
    Data(ProposalData),
    Fin(ProposalFin),
}

impl ProposalPart {
    pub fn get_type(&self) -> &'static str {
        match self {
            Self::Init(_) => "init",
            Self::Data(_) => "data",
            Self::Fin(_) => "fin",
        }
    }

    pub fn as_init(&self) -> Option<&ProposalInit> {
        match self {
            Self::Init(i) => Some(i),
            _ => None,
        }
    }

    pub fn as_data(&self) -> Option<&ProposalData> {
        match self {
            Self::Data(d) => Some(d),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn as_fin(&self) -> Option<&ProposalFin> {
        match self {
            Self::Fin(f) => Some(f),
            _ => None,
        }
    }
}

impl malachitebft_core_types::ProposalPart<EthexeContext> for ProposalPart {
    fn is_first(&self) -> bool {
        matches!(self, Self::Init(_))
    }

    fn is_last(&self) -> bool {
        matches!(self, Self::Fin(_))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalInit {
    pub height: Height,
    pub round: Round,
    pub pol_round: Round,
    pub proposer: Address,
}

impl ProposalInit {
    pub fn new(height: Height, round: Round, pol_round: Round, proposer: Address) -> Self {
        Self {
            height,
            round,
            pol_round,
            proposer,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalData {
    pub block: SequencerBlock,
}

impl ProposalData {
    pub fn new(block: SequencerBlock) -> Self {
        Self { block }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalFin {
    pub signature: Signature,
}

impl ProposalFin {
    pub fn new(signature: Signature) -> Self {
        Self { signature }
    }
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

/// Context type tying all of the above together.
#[derive(Clone, Debug, Default)]
pub struct EthexeContext;

impl EthexeContext {
    pub fn new() -> Self {
        Self
    }
}

impl Context for EthexeContext {
    type Address = Address;
    type Height = Height;
    type ProposalPart = ProposalPart;
    type Proposal = Proposal;
    type Validator = Validator;
    type ValidatorSet = ValidatorSet;
    type Value = Value;
    type Vote = Vote;
    type Extension = Bytes;
    type SigningScheme = K256;
    type Timeouts = LinearTimeouts;

    fn select_proposer<'a>(
        &self,
        validator_set: &'a Self::ValidatorSet,
        height: Self::Height,
        round: Round,
    ) -> &'a Self::Validator {
        assert!(validator_set.count() > 0);
        assert!(round != Round::Nil && round.as_i64() >= 0);

        let proposer_index = {
            let h = height.as_u64() as usize;
            let r = round.as_i64() as usize;
            (h.saturating_sub(1) + r) % validator_set.count()
        };

        validator_set
            .get_by_index(proposer_index)
            .expect("proposer_index is in-range")
    }

    fn new_proposal(
        &self,
        height: Height,
        round: Round,
        value: Value,
        pol_round: Round,
        address: Address,
    ) -> Proposal {
        Proposal::new(height, round, value, pol_round, address)
    }

    fn new_prevote(
        &self,
        height: Height,
        round: Round,
        value_id: NilOrVal<ValueId>,
        address: Address,
    ) -> Vote {
        Vote::new_prevote(height, round, value_id, address)
    }

    fn new_precommit(
        &self,
        height: Height,
        round: Round,
        value_id: NilOrVal<ValueId>,
        address: Address,
    ) -> Vote {
        Vote::new_precommit(height, round, value_id, address)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Sign the fin part of a streamed proposal over a keccak256 of
/// (height_be, round_be, data-bytes).
pub fn sign_proposal_fin(
    signer: &EthexeSigner,
    height: Height,
    round: Round,
    data_bytes: &[u8],
) -> Signature {
    let mut h = Keccak256::new();
    h.update(height.as_u64().to_be_bytes());
    h.update(round.as_i64().to_be_bytes());
    h.update(data_bytes);
    let hash = h.finalize();
    signer.sign(&hash)
}

/// Flatten accumulated extensions from previous height's commit
/// certificate into a single byte buffer (for inclusion in the next
/// `Value`).
#[allow(dead_code)]
pub fn flatten_extensions(certs: Vec<SignedExtension<EthexeContext>>) -> Vec<u8> {
    let mut out = Vec::new();
    for e in certs {
        out.extend_from_slice(e.message.as_ref());
    }
    out
}

// ---------------------------------------------------------------------------
// Signing provider — secp256k1/ECDSA (k256 via malachitebft-signing-ecdsa).
// ---------------------------------------------------------------------------

/// Node-local signing provider. Owns an ECDSA [`PrivateKey`] on the
/// k256 curve; the raw 32-byte secret is the same one that identifies
/// the node on-chain (extracted once at service startup from
/// [`gsigner::Signer`]), so Malachite votes and on-chain commitments
/// share a single validator identity.
#[derive(Debug)]
pub struct EthexeSigner {
    private_key: PrivateKey,
}

impl EthexeSigner {
    pub fn new(private_key: PrivateKey) -> Self {
        Self { private_key }
    }

    #[allow(dead_code)]
    pub fn private_key(&self) -> &PrivateKey {
        &self.private_key
    }

    #[allow(dead_code)]
    pub fn public_key(&self) -> PublicKey {
        self.private_key.public_key()
    }

    pub fn sign(&self, data: &[u8]) -> Signature {
        self.private_key.sign(data)
    }

    pub fn verify(&self, data: &[u8], signature: &Signature, public_key: &PublicKey) -> bool {
        public_key.verify(data, signature).is_ok()
    }
}

#[async_trait]
impl SigningProvider<EthexeContext> for EthexeSigner {
    async fn sign_bytes(&self, bytes: &[u8]) -> Result<Signature, SigningError> {
        Ok(self.sign(bytes))
    }

    async fn verify_signed_bytes(
        &self,
        bytes: &[u8],
        signature: &Signature,
        public_key: &PublicKey,
    ) -> Result<VerificationResult, SigningError> {
        Ok(VerificationResult::from_bool(
            self.verify(bytes, signature, public_key),
        ))
    }

    async fn sign_vote(&self, vote: Vote) -> Result<SignedVote<EthexeContext>, SigningError> {
        let signature = self.sign(&vote.to_sign_bytes());
        Ok(SignedVote::new(vote, signature))
    }

    async fn verify_signed_vote(
        &self,
        vote: &Vote,
        signature: &Signature,
        public_key: &PublicKey,
    ) -> Result<VerificationResult, SigningError> {
        Ok(VerificationResult::from_bool(
            public_key.verify(&vote.to_sign_bytes(), signature).is_ok(),
        ))
    }

    async fn sign_proposal(
        &self,
        proposal: Proposal,
    ) -> Result<SignedProposal<EthexeContext>, SigningError> {
        let signature = self.sign(&proposal.to_sign_bytes());
        Ok(SignedProposal::new(proposal, signature))
    }

    async fn verify_signed_proposal(
        &self,
        proposal: &Proposal,
        signature: &Signature,
        public_key: &PublicKey,
    ) -> Result<VerificationResult, SigningError> {
        Ok(VerificationResult::from_bool(
            public_key
                .verify(&proposal.to_sign_bytes(), signature)
                .is_ok(),
        ))
    }

    async fn sign_vote_extension(
        &self,
        extension: Bytes,
    ) -> Result<SignedExtension<EthexeContext>, SigningError> {
        let signature = self.sign(extension.as_ref());
        Ok(SignedMessage::new(extension, signature))
    }

    async fn verify_signed_vote_extension(
        &self,
        extension: &Bytes,
        signature: &Signature,
        public_key: &PublicKey,
    ) -> Result<VerificationResult, SigningError> {
        Ok(VerificationResult::from_bool(
            public_key.verify(extension.as_ref(), signature).is_ok(),
        ))
    }
}
