// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Concrete `malachitebft_core_types::Context` implementation.
//!
//! Application-agnostic by design: every concrete type below is non-
//! generic. The application's payload only travels on the wire as
//! the SCALE-encoded [`crate::Block`] (see [`Value`]); the
//! engine never sees the application's payload type directly.
//!
//! The malachite-side [`ValueId`] is a 32-byte content hash of the
//! [`Value`] payload — keccak256 over a domain tag and the encoded
//! block bytes. The application-side block hash ([`crate::H256`],
//! computed via [`crate::Block::hash`]) is a separate identity used
//! by the service / [`crate::Externalities`].

use core::slice;
use std::{
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use async_trait::async_trait;
use bytes::Bytes;
use parity_scale_codec::{Decode, Encode, Error as CodecError, Input, Output};
use serde::{Deserialize, Serialize};
use sha3::{Digest as _, Keccak256};

use malachitebft_core_types::{
    Context, LinearTimeouts, NilOrVal, Round, SignedExtension, SignedMessage, SignedProposal,
    SignedVote, ValidatorSet as _ValidatorSetTrait, Value as _ValueTrait, VoteType, VotingPower,
};
use malachitebft_signing::{Error as SigningError, SigningProvider, VerificationResult};
use malachitebft_signing_ecdsa::K256;

pub use malachitebft_test::Height;

use crate::{
    signing::{MalachiteSigner, PublicKey, Signature, signature_from_vec, signature_to_vec},
    types::Address,
};

// Address — adopt the foreign trait via our local newtype.
impl malachitebft_core_types::Address for Address {}

/// On-the-wire value. The block travels as opaque bytes
/// (SCALE-encoded [`crate::Block`]) so the consensus types stay free
/// of the application's payload trait bounds.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Value {
    pub block_bytes: Vec<u8>,
}

impl Value {
    pub fn new(block_bytes: Vec<u8>) -> Self {
        Self { block_bytes }
    }

    pub fn size_bytes(&self) -> usize {
        self.block_bytes.len()
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Value {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id().0.cmp(&other.id().0)
    }
}

impl malachitebft_core_types::Value for Value {
    type Id = ValueId;

    fn id(&self) -> Self::Id {
        let mut h = Keccak256::new();
        h.update(b"mala-svc/value-id:v1:");
        h.update(&self.block_bytes);
        let out = h.finalize();
        ValueId(out.into())
    }
}

/// 32-byte content-addressed identifier for a [`Value`].
#[derive(Copy, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Encode, Decode)]
pub struct ValueId(pub [u8; 32]);

impl ValueId {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Display for ValueId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

impl fmt::Debug for ValueId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "ValueId({self})")
    }
}

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

impl malachitebft_core_types::Validator<MalachiteCtx> for Validator {
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
}

impl malachitebft_core_types::ValidatorSet<MalachiteCtx> for ValidatorSet {
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vote {
    pub typ: VoteType,
    pub height: Height,
    pub round: Round,
    pub value: NilOrVal<ValueId>,
    pub validator_address: Address,
    /// Vote extensions are not serialized — they are kept as `None`
    /// so the SCALE round-trip stays canonical for the
    /// `to_sign_bytes` / `from_sign_bytes` flow.
    pub extension: Option<SignedExtension<MalachiteCtx>>,
}

impl Encode for Vote {
    fn encode_to<W: Output + ?Sized>(&self, dest: &mut W) {
        encode_vote_type_to(self.typ, dest);
        self.height.as_u64().encode_to(dest);
        encode_round_to(self.round, dest);
        encode_nil_or_val_value_id_to(&self.value, dest);
        encode_address_to(&self.validator_address, dest);
    }
}

impl Decode for Vote {
    fn decode<I: Input>(input: &mut I) -> Result<Self, CodecError> {
        let typ = decode_vote_type(input)?;
        let height = Height::new(u64::decode(input)?);
        let round = decode_round(input)?;
        let value = decode_nil_or_val_value_id(input)?;
        let validator_address = decode_address(input)?;
        Ok(Self {
            typ,
            height,
            round,
            value,
            validator_address,
            extension: None,
        })
    }
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
        Encode::encode(self).into()
    }

    pub fn from_sign_bytes(bytes: &[u8]) -> Result<Self, CodecError> {
        Self::decode(&mut &bytes[..])
    }
}

impl malachitebft_core_types::Vote<MalachiteCtx> for Vote {
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

    fn extension(&self) -> Option<&SignedExtension<MalachiteCtx>> {
        self.extension.as_ref()
    }

    fn take_extension(&mut self) -> Option<SignedExtension<MalachiteCtx>> {
        self.extension.take()
    }

    fn extend(self, extension: SignedExtension<MalachiteCtx>) -> Self {
        Self {
            extension: Some(extension),
            ..self
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub height: Height,
    pub round: Round,
    pub value: Value,
    pub pol_round: Round,
    pub proposer: Address,
}

impl Encode for Proposal {
    fn encode_to<W: Output + ?Sized>(&self, dest: &mut W) {
        self.height.as_u64().encode_to(dest);
        encode_round_to(self.round, dest);
        self.value.encode_to(dest);
        encode_round_to(self.pol_round, dest);
        encode_address_to(&self.proposer, dest);
    }
}

impl Decode for Proposal {
    fn decode<I: Input>(input: &mut I) -> Result<Self, CodecError> {
        let height = Height::new(u64::decode(input)?);
        let round = decode_round(input)?;
        let value = Value::decode(input)?;
        let pol_round = decode_round(input)?;
        let proposer = decode_address(input)?;
        Ok(Self {
            height,
            round,
            value,
            pol_round,
            proposer,
        })
    }
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
        Encode::encode(self).into()
    }

    pub fn from_sign_bytes(bytes: &[u8]) -> Result<Self, CodecError> {
        Self::decode(&mut &bytes[..])
    }
}

impl malachitebft_core_types::Proposal<MalachiteCtx> for Proposal {
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

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
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
}

impl malachitebft_core_types::ProposalPart<MalachiteCtx> for ProposalPart {
    fn is_first(&self) -> bool {
        matches!(self, Self::Init(_))
    }

    fn is_last(&self) -> bool {
        matches!(self, Self::Fin(_))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

impl Encode for ProposalInit {
    fn encode_to<W: Output + ?Sized>(&self, dest: &mut W) {
        self.height.as_u64().encode_to(dest);
        encode_round_to(self.round, dest);
        encode_round_to(self.pol_round, dest);
        encode_address_to(&self.proposer, dest);
    }
}

impl Decode for ProposalInit {
    fn decode<I: Input>(input: &mut I) -> Result<Self, CodecError> {
        let height = Height::new(u64::decode(input)?);
        let round = decode_round(input)?;
        let pol_round = decode_round(input)?;
        let proposer = decode_address(input)?;
        Ok(Self {
            height,
            round,
            pol_round,
            proposer,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct ProposalData {
    pub block_bytes: Vec<u8>,
}

impl ProposalData {
    pub fn new(block_bytes: Vec<u8>) -> Self {
        Self { block_bytes }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposalFin {
    pub signature: Signature,
}

impl ProposalFin {
    pub fn new(signature: Signature) -> Self {
        Self { signature }
    }
}

impl Encode for ProposalFin {
    fn encode_to<W: Output + ?Sized>(&self, dest: &mut W) {
        encode_signature_to(&self.signature, dest);
    }
}

impl Decode for ProposalFin {
    fn decode<I: Input>(input: &mut I) -> Result<Self, CodecError> {
        Ok(Self {
            signature: decode_signature(input)?,
        })
    }
}

/// Concrete malachite [`Context`] for the `ethexe-malachite-core` crate.
#[derive(Clone, Debug, Default)]
pub struct MalachiteCtx;

impl MalachiteCtx {
    pub fn new() -> Self {
        Self
    }
}

impl Context for MalachiteCtx {
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

#[async_trait]
impl SigningProvider<MalachiteCtx> for MalachiteSigner {
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

    async fn sign_vote(&self, vote: Vote) -> Result<SignedVote<MalachiteCtx>, SigningError> {
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
    ) -> Result<SignedProposal<MalachiteCtx>, SigningError> {
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
    ) -> Result<SignedExtension<MalachiteCtx>, SigningError> {
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Sign the `Fin` part of a streamed proposal over
/// `keccak256(height_be || round_be || data_bytes)`.
pub fn sign_proposal_fin(
    signer: &MalachiteSigner,
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

fn encode_round_to<W: Output + ?Sized>(round: Round, dest: &mut W) {
    round.as_i64().encode_to(dest);
}

fn decode_round<I: Input>(input: &mut I) -> Result<Round, CodecError> {
    let v = i64::decode(input)?;
    if v == -1 {
        Ok(Round::Nil)
    } else if v >= 0 && v <= u32::MAX as i64 {
        Ok(Round::new(v as u32))
    } else {
        Err(CodecError::from("Round out of range"))
    }
}

fn encode_address_to<W: Output + ?Sized>(addr: &Address, dest: &mut W) {
    addr.0.0.encode_to(dest);
}

fn decode_address<I: Input>(input: &mut I) -> Result<Address, CodecError> {
    let bytes = <[u8; 20]>::decode(input)?;
    Ok(Address::from_inner(gsigner::schemes::secp256k1::Address(
        bytes,
    )))
}

fn encode_signature_to<W: Output + ?Sized>(sig: &Signature, dest: &mut W) {
    signature_to_vec(sig).encode_to(dest);
}

fn decode_signature<I: Input>(input: &mut I) -> Result<Signature, CodecError> {
    let bytes = Vec::<u8>::decode(input)?;
    signature_from_vec(&bytes)
        .map_err(|e| CodecError::from("invalid signature").chain(e.to_string()))
}

fn encode_nil_or_val_value_id_to<W: Output + ?Sized>(v: &NilOrVal<ValueId>, dest: &mut W) {
    match v {
        NilOrVal::Nil => 0u8.encode_to(dest),
        NilOrVal::Val(id) => {
            1u8.encode_to(dest);
            id.0.encode_to(dest);
        }
    }
}

fn decode_nil_or_val_value_id<I: Input>(input: &mut I) -> Result<NilOrVal<ValueId>, CodecError> {
    let tag = u8::decode(input)?;
    match tag {
        0 => Ok(NilOrVal::Nil),
        1 => {
            let bytes = <[u8; 32]>::decode(input)?;
            Ok(NilOrVal::Val(ValueId(bytes)))
        }
        _ => Err(CodecError::from("invalid NilOrVal tag")),
    }
}

fn encode_vote_type_to<W: Output + ?Sized>(t: VoteType, dest: &mut W) {
    let b: u8 = match t {
        VoteType::Prevote => 0,
        VoteType::Precommit => 1,
    };
    b.encode_to(dest);
}

fn decode_vote_type<I: Input>(input: &mut I) -> Result<VoteType, CodecError> {
    match u8::decode(input)? {
        0 => Ok(VoteType::Prevote),
        1 => Ok(VoteType::Precommit),
        _ => Err(CodecError::from("invalid VoteType tag")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing::private_key_from_bytes;
    use proptest::prelude::*;

    fn mk_keypair(seed: u8) -> (PublicKey, MalachiteSigner) {
        let mut bytes = [0u8; 32];
        bytes[31] = seed;
        let priv_key = private_key_from_bytes(&bytes).unwrap();
        let pk = priv_key.public_key();
        (pk, MalachiteSigner::new(priv_key))
    }

    #[test]
    fn validator_set_is_sorted_by_address() {
        let (pk_a, _) = mk_keypair(1);
        let (pk_b, _) = mk_keypair(2);
        let (pk_c, _) = mk_keypair(3);
        let v_a = Validator::new(pk_a.clone(), 1);
        let v_b = Validator::new(pk_b.clone(), 1);
        let v_c = Validator::new(pk_c.clone(), 1);
        let unsorted = vec![v_b.clone(), v_c.clone(), v_a.clone()];
        let vs = ValidatorSet::new(unsorted);
        let addrs: Vec<_> = vs.iter().map(|v| v.address).collect();
        let mut sorted = vec![v_a.address, v_b.address, v_c.address];
        sorted.sort();
        assert_eq!(addrs, sorted);
    }

    #[test]
    fn select_proposer_round_robin_by_height() {
        let validators: Vec<_> = (1u8..=4)
            .map(|s| Validator::new(mk_keypair(s).0, 1))
            .collect();
        let vs = ValidatorSet::new(validators);
        let ctx = MalachiteCtx::new();
        let h1 = ctx.select_proposer(&vs, Height::new(1), Round::new(0));
        let h2 = ctx.select_proposer(&vs, Height::new(2), Round::new(0));
        let h5 = ctx.select_proposer(&vs, Height::new(5), Round::new(0));
        // height-1 vs height-5 with set size 4 gives the same index.
        assert_eq!(h1.address, h5.address);
        assert_ne!(h1.address, h2.address);
    }

    #[test]
    fn select_proposer_advances_with_round() {
        let validators: Vec<_> = (1u8..=4)
            .map(|s| Validator::new(mk_keypair(s).0, 1))
            .collect();
        let vs = ValidatorSet::new(validators);
        let ctx = MalachiteCtx::new();
        let r0 = ctx.select_proposer(&vs, Height::new(1), Round::new(0));
        let r1 = ctx.select_proposer(&vs, Height::new(1), Round::new(1));
        assert_ne!(r0.address, r1.address);
    }

    #[test]
    fn value_id_is_content_addressed() {
        let v1 = Value::new(b"abc".to_vec());
        let v2 = Value::new(b"abc".to_vec());
        let v3 = Value::new(b"xyz".to_vec());
        assert_eq!(v1.id(), v2.id());
        assert_ne!(v1.id(), v3.id());
    }

    #[test]
    fn vote_signature_round_trip() {
        let (pk, signer) = mk_keypair(7);
        let addr = Address::from_public_key(&pk);
        let vote = Vote::new_prevote(Height::new(1), Round::new(0), NilOrVal::Nil, addr);
        let bytes = vote.to_sign_bytes();
        let sig = signer.sign(&bytes);
        assert!(signer.verify(&bytes, &sig, &pk));
    }

    #[test]
    fn proposal_signature_round_trip() {
        let (pk, signer) = mk_keypair(8);
        let addr = Address::from_public_key(&pk);
        let proposal = Proposal::new(
            Height::new(1),
            Round::new(0),
            Value::new(b"some block".to_vec()),
            Round::Nil,
            addr,
        );
        let bytes = proposal.to_sign_bytes();
        let sig = signer.sign(&bytes);
        assert!(signer.verify(&bytes, &sig, &pk));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn prop_value_id_deterministic(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
            let v1 = Value::new(bytes.clone());
            let v2 = Value::new(bytes);
            prop_assert_eq!(v1.id(), v2.id());
        }

        #[test]
        fn prop_value_id_distinct_per_payload(
            a in proptest::collection::vec(any::<u8>(), 1..128),
            b in proptest::collection::vec(any::<u8>(), 1..128),
        ) {
            prop_assume!(a != b);
            prop_assert_ne!(Value::new(a).id(), Value::new(b).id());
        }
    }
}
