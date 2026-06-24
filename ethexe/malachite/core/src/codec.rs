// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! SCALE wire codec for the malachite engine.
//!
//! Malachite's internal types are generic over `Context` and don't
//! derive serialization directly — we declare local `Raw*` wrapper
//! types that derive `parity_scale_codec::{Encode, Decode}` and
//! provide `From` (encode side) / `From` or `TryFrom` (decode side)
//! conversions. `TryFrom` is used wherever decode can fail on
//! malformed peer input — invalid signatures, bad peer-ids,
//! out-of-range rounds — so a malicious peer can't panic the engine.
//!
//! Compared to a JSON codec the SCALE encoding is roughly 2-3x
//! smaller on the wire and faster to serialize/deserialize, plus it
//! gives a fully canonical byte form (no whitespace / map-ordering
//! ambiguity) which is what we want for `Vote::to_sign_bytes` and
//! `Proposal::to_sign_bytes`.

use bytes::Bytes;
use parity_scale_codec::{Decode, Encode, Error as CodecError};

use malachitebft_app::streaming::StreamId;
use malachitebft_codec::{Codec, HasEncodedLen};
use malachitebft_core_consensus::{LivenessMsg, ProposedValue, SignedConsensusMsg};
use malachitebft_core_types::{
    CommitCertificate, CommitSignature, NilOrVal, PolkaCertificate, PolkaSignature, Round,
    RoundCertificate, RoundCertificateType, RoundSignature, SignedProposal, SignedVote,
    ValidatorProof, Validity, VoteType,
};
use malachitebft_engine::util::streaming::{StreamContent, StreamMessage};
use malachitebft_sync::{
    PeerId, RawDecidedValue, Request, Response, Status, ValueRequest, ValueResponse,
};

use crate::{
    context::{Height, MalachiteCtx, Proposal, ProposalPart, Value, ValueId, Vote},
    signing::{Signature, signature_from_vec, signature_to_vec},
    types::Address,
};

/// SCALE codec for malachite wire types. Zero-sized handle.
#[derive(Copy, Clone, Debug, Default)]
pub struct ScaleCodec;

// ---------------------------------------------------------------------------
// Codec impls
// ---------------------------------------------------------------------------

impl Codec<Value> for ScaleCodec {
    type Error = CodecError;
    fn decode(&self, bytes: Bytes) -> Result<Value, Self::Error> {
        Value::decode(&mut &bytes[..])
    }
    fn encode(&self, msg: &Value) -> Result<Bytes, Self::Error> {
        Ok(Bytes::from(Encode::encode(msg)))
    }
}

impl Codec<ProposalPart> for ScaleCodec {
    type Error = CodecError;
    fn decode(&self, bytes: Bytes) -> Result<ProposalPart, Self::Error> {
        ProposalPart::decode(&mut &bytes[..])
    }
    fn encode(&self, msg: &ProposalPart) -> Result<Bytes, Self::Error> {
        Ok(Bytes::from(Encode::encode(msg)))
    }
}

impl Codec<SignedConsensusMsg<MalachiteCtx>> for ScaleCodec {
    type Error = CodecError;
    fn decode(&self, bytes: Bytes) -> Result<SignedConsensusMsg<MalachiteCtx>, Self::Error> {
        let raw = RawSignedConsensusMsg::decode(&mut &bytes[..])?;
        SignedConsensusMsg::try_from(raw)
    }
    fn encode(&self, msg: &SignedConsensusMsg<MalachiteCtx>) -> Result<Bytes, Self::Error> {
        Ok(Bytes::from(Encode::encode(&RawSignedConsensusMsg::from(
            msg.clone(),
        ))))
    }
}

impl Codec<StreamMessage<ProposalPart>> for ScaleCodec {
    type Error = CodecError;
    fn decode(&self, bytes: Bytes) -> Result<StreamMessage<ProposalPart>, Self::Error> {
        let raw = RawStreamMessage::decode(&mut &bytes[..])?;
        Ok(StreamMessage::from(raw))
    }
    fn encode(&self, msg: &StreamMessage<ProposalPart>) -> Result<Bytes, Self::Error> {
        Ok(Bytes::from(Encode::encode(&RawStreamMessage::from(
            msg.clone(),
        ))))
    }
}

impl Codec<Status<MalachiteCtx>> for ScaleCodec {
    type Error = CodecError;
    fn decode(&self, bytes: Bytes) -> Result<Status<MalachiteCtx>, Self::Error> {
        let raw = RawStatus::decode(&mut &bytes[..])?;
        Status::try_from(raw)
    }
    fn encode(&self, msg: &Status<MalachiteCtx>) -> Result<Bytes, Self::Error> {
        Ok(Bytes::from(Encode::encode(&RawStatus::from(msg.clone()))))
    }
}

impl Codec<Request<MalachiteCtx>> for ScaleCodec {
    type Error = CodecError;
    fn decode(&self, bytes: Bytes) -> Result<Request<MalachiteCtx>, Self::Error> {
        let raw = RawRequest::decode(&mut &bytes[..])?;
        Ok(Request::from(raw))
    }
    fn encode(&self, msg: &Request<MalachiteCtx>) -> Result<Bytes, Self::Error> {
        Ok(Bytes::from(Encode::encode(&RawRequest::from(msg.clone()))))
    }
}

impl Codec<Response<MalachiteCtx>> for ScaleCodec {
    type Error = CodecError;
    fn decode(&self, bytes: Bytes) -> Result<Response<MalachiteCtx>, Self::Error> {
        let raw = RawResponse::decode(&mut &bytes[..])?;
        Response::try_from(raw)
    }
    fn encode(&self, msg: &Response<MalachiteCtx>) -> Result<Bytes, Self::Error> {
        Ok(Bytes::from(Encode::encode(&RawResponse::from(msg.clone()))))
    }
}

impl HasEncodedLen<Response<MalachiteCtx>> for ScaleCodec {
    fn encoded_len(
        &self,
        msg: &Response<MalachiteCtx>,
    ) -> Result<usize, <Self as Codec<Response<MalachiteCtx>>>::Error> {
        Ok(Encode::encoded_size(&RawResponse::from(msg.clone())))
    }
}

impl Codec<LivenessMsg<MalachiteCtx>> for ScaleCodec {
    type Error = CodecError;
    fn decode(&self, bytes: Bytes) -> Result<LivenessMsg<MalachiteCtx>, Self::Error> {
        let raw = RawLivenessMsg::decode(&mut &bytes[..])?;
        LivenessMsg::try_from(raw)
    }
    fn encode(&self, msg: &LivenessMsg<MalachiteCtx>) -> Result<Bytes, Self::Error> {
        Ok(Bytes::from(Encode::encode(&RawLivenessMsg::from(
            msg.clone(),
        ))))
    }
}

impl Codec<ValidatorProof<MalachiteCtx>> for ScaleCodec {
    type Error = CodecError;
    fn decode(&self, bytes: Bytes) -> Result<ValidatorProof<MalachiteCtx>, Self::Error> {
        let raw = RawValidatorProof::decode(&mut &bytes[..])?;
        ValidatorProof::try_from(raw)
    }
    fn encode(&self, msg: &ValidatorProof<MalachiteCtx>) -> Result<Bytes, Self::Error> {
        Ok(Bytes::from(Encode::encode(&RawValidatorProof::from(
            msg.clone(),
        ))))
    }
}

impl Codec<ProposedValue<MalachiteCtx>> for ScaleCodec {
    type Error = CodecError;
    fn decode(&self, bytes: Bytes) -> Result<ProposedValue<MalachiteCtx>, Self::Error> {
        RawProposedValue::decode(&mut &bytes[..])?.try_into()
    }
    fn encode(&self, msg: &ProposedValue<MalachiteCtx>) -> Result<Bytes, Self::Error> {
        Ok(Bytes::from(Encode::encode(&RawProposedValue::from(
            msg.clone(),
        ))))
    }
}

impl Codec<CommitCertificate<MalachiteCtx>> for ScaleCodec {
    type Error = CodecError;
    fn decode(&self, bytes: Bytes) -> Result<CommitCertificate<MalachiteCtx>, Self::Error> {
        let raw = RawCommitCertificate::decode(&mut &bytes[..])?;
        CommitCertificate::try_from(raw)
    }
    fn encode(&self, msg: &CommitCertificate<MalachiteCtx>) -> Result<Bytes, Self::Error> {
        Ok(Bytes::from(Encode::encode(&RawCommitCertificate::from(
            msg.clone(),
        ))))
    }
}

// ---------------------------------------------------------------------------
// Raw wrapper types (SCALE-derive)
// ---------------------------------------------------------------------------

#[derive(Encode, Decode)]
struct RawSignature(Vec<u8>);

impl From<&Signature> for RawSignature {
    fn from(s: &Signature) -> Self {
        Self(signature_to_vec(s))
    }
}

impl TryFrom<RawSignature> for Signature {
    type Error = CodecError;
    fn try_from(r: RawSignature) -> Result<Self, Self::Error> {
        signature_from_vec(&r.0)
            .map_err(|e| CodecError::from("invalid signature bytes").chain(e.to_string()))
    }
}

#[derive(Encode, Decode)]
struct RawAddress([u8; 20]);

impl From<&Address> for RawAddress {
    fn from(a: &Address) -> Self {
        Self(a.0.0)
    }
}

impl From<RawAddress> for Address {
    fn from(r: RawAddress) -> Self {
        Address::from_inner(gsigner::schemes::secp256k1::Address(r.0))
    }
}

#[derive(Encode, Decode)]
struct RawSignedMessage {
    message: Vec<u8>,
    signature: RawSignature,
}

#[derive(Encode, Decode)]
enum RawSignedConsensusMsg {
    Vote(RawSignedMessage),
    Proposal(RawSignedMessage),
}

impl From<SignedConsensusMsg<MalachiteCtx>> for RawSignedConsensusMsg {
    fn from(value: SignedConsensusMsg<MalachiteCtx>) -> Self {
        match value {
            SignedConsensusMsg::Vote(vote) => Self::Vote(RawSignedMessage {
                message: vote.message.to_sign_bytes().to_vec(),
                signature: RawSignature::from(&vote.signature),
            }),
            SignedConsensusMsg::Proposal(proposal) => Self::Proposal(RawSignedMessage {
                message: proposal.message.to_sign_bytes().to_vec(),
                signature: RawSignature::from(&proposal.signature),
            }),
        }
    }
}

impl TryFrom<RawSignedConsensusMsg> for SignedConsensusMsg<MalachiteCtx> {
    type Error = CodecError;
    fn try_from(value: RawSignedConsensusMsg) -> Result<Self, Self::Error> {
        match value {
            RawSignedConsensusMsg::Vote(raw) => Ok(SignedConsensusMsg::Vote(SignedVote {
                message: Vote::from_sign_bytes(&raw.message)?,
                signature: Signature::try_from(raw.signature)?,
            })),
            RawSignedConsensusMsg::Proposal(raw) => {
                Ok(SignedConsensusMsg::Proposal(SignedProposal {
                    message: Proposal::from_sign_bytes(&raw.message)?,
                    signature: Signature::try_from(raw.signature)?,
                }))
            }
        }
    }
}

#[derive(Encode, Decode)]
struct RawStreamMessage {
    stream_id: Vec<u8>,
    sequence: u64,
    content: RawStreamContent,
}

#[derive(Encode, Decode)]
enum RawStreamContent {
    Data(ProposalPart),
    Fin,
}

impl From<StreamMessage<ProposalPart>> for RawStreamMessage {
    fn from(value: StreamMessage<ProposalPart>) -> Self {
        Self {
            stream_id: value.stream_id.to_bytes().to_vec(),
            sequence: value.sequence,
            content: match value.content {
                StreamContent::Data(part) => RawStreamContent::Data(part),
                StreamContent::Fin => RawStreamContent::Fin,
            },
        }
    }
}

impl From<RawStreamMessage> for StreamMessage<ProposalPart> {
    fn from(value: RawStreamMessage) -> Self {
        Self {
            stream_id: StreamId::new(Bytes::from(value.stream_id)),
            sequence: value.sequence,
            content: match value.content {
                RawStreamContent::Data(part) => StreamContent::Data(part),
                RawStreamContent::Fin => StreamContent::Fin,
            },
        }
    }
}

#[derive(Encode, Decode)]
struct RawStatus {
    peer_id: Vec<u8>,
    tip_height: u64,
    history_min_height: u64,
}

impl From<Status<MalachiteCtx>> for RawStatus {
    fn from(value: Status<MalachiteCtx>) -> Self {
        Self {
            peer_id: value.peer_id.to_bytes(),
            tip_height: value.tip_height.as_u64(),
            history_min_height: value.history_min_height.as_u64(),
        }
    }
}

impl TryFrom<RawStatus> for Status<MalachiteCtx> {
    type Error = CodecError;
    fn try_from(value: RawStatus) -> Result<Self, Self::Error> {
        let peer_id = PeerId::from_bytes(&value.peer_id)
            .map_err(|e| CodecError::from("invalid peer-id in Status").chain(e.to_string()))?;
        Ok(Self {
            peer_id,
            tip_height: Height::new(value.tip_height),
            history_min_height: Height::new(value.history_min_height),
        })
    }
}

#[derive(Encode, Decode)]
struct ValueRawRequest {
    height: u64,
    end_height: Option<u64>,
}

#[derive(Encode, Decode)]
enum RawRequest {
    SyncRequest(ValueRawRequest),
}

impl From<Request<MalachiteCtx>> for RawRequest {
    fn from(value: Request<MalachiteCtx>) -> Self {
        match value {
            Request::ValueRequest(request) => Self::SyncRequest(ValueRawRequest {
                height: request.range.start().as_u64(),
                end_height: Some(request.range.end().as_u64()),
            }),
        }
    }
}

impl From<RawRequest> for Request<MalachiteCtx> {
    fn from(value: RawRequest) -> Self {
        match value {
            RawRequest::SyncRequest(raw) => {
                let start = Height::new(raw.height);
                let end = Height::new(raw.end_height.unwrap_or(raw.height));
                Self::ValueRequest(ValueRequest { range: start..=end })
            }
        }
    }
}

#[derive(Encode, Decode)]
struct RawCommitSignature {
    address: RawAddress,
    signature: RawSignature,
}

#[derive(Encode, Decode)]
struct RawCommitCertificate {
    height: u64,
    round: i64,
    value_id: [u8; 32],
    commit_signatures: Vec<RawCommitSignature>,
}

impl TryFrom<RawCommitCertificate> for CommitCertificate<MalachiteCtx> {
    type Error = CodecError;
    fn try_from(value: RawCommitCertificate) -> Result<Self, Self::Error> {
        let mut commit_signatures = Vec::with_capacity(value.commit_signatures.len());
        for sig in value.commit_signatures {
            commit_signatures.push(CommitSignature {
                address: Address::from(sig.address),
                signature: Signature::try_from(sig.signature)?,
            });
        }
        Ok(CommitCertificate {
            height: Height::new(value.height),
            round: i64_to_round(value.round)?,
            value_id: ValueId(value.value_id),
            commit_signatures,
        })
    }
}

impl From<CommitCertificate<MalachiteCtx>> for RawCommitCertificate {
    fn from(value: CommitCertificate<MalachiteCtx>) -> Self {
        Self {
            height: value.height.as_u64(),
            round: round_to_i64(value.round),
            value_id: value.value_id.0,
            commit_signatures: value
                .commit_signatures
                .iter()
                .map(|sig| RawCommitSignature {
                    address: RawAddress::from(&sig.address),
                    signature: RawSignature::from(&sig.signature),
                })
                .collect(),
        }
    }
}

#[derive(Encode, Decode)]
struct RawSyncedValue {
    value_bytes: Vec<u8>,
    certificate: RawCommitCertificate,
}

#[derive(Encode, Decode)]
struct ValueRawResponse {
    start_height: u64,
    value: Vec<RawSyncedValue>,
}

impl From<ValueResponse<MalachiteCtx>> for ValueRawResponse {
    fn from(response: ValueResponse<MalachiteCtx>) -> Self {
        Self {
            start_height: response.start_height.as_u64(),
            value: response
                .values
                .into_iter()
                .map(|v| RawSyncedValue {
                    value_bytes: v.value_bytes.to_vec(),
                    certificate: v.certificate.into(),
                })
                .collect(),
        }
    }
}

impl TryFrom<ValueRawResponse> for ValueResponse<MalachiteCtx> {
    type Error = CodecError;
    fn try_from(response: ValueRawResponse) -> Result<Self, Self::Error> {
        let mut values = Vec::with_capacity(response.value.len());
        for v in response.value {
            values.push(RawDecidedValue {
                value_bytes: Bytes::from(v.value_bytes),
                certificate: CommitCertificate::try_from(v.certificate)?,
            });
        }
        Ok(Self {
            start_height: Height::new(response.start_height),
            values,
        })
    }
}

#[derive(Encode, Decode)]
enum RawResponse {
    ValueResponse(ValueRawResponse),
}

impl From<Response<MalachiteCtx>> for RawResponse {
    fn from(value: Response<MalachiteCtx>) -> Self {
        match value {
            Response::ValueResponse(resp) => Self::ValueResponse(resp.into()),
        }
    }
}

impl TryFrom<RawResponse> for Response<MalachiteCtx> {
    type Error = CodecError;
    fn try_from(value: RawResponse) -> Result<Self, Self::Error> {
        Ok(match value {
            RawResponse::ValueResponse(resp) => Self::ValueResponse(ValueResponse::try_from(resp)?),
        })
    }
}

#[derive(Encode, Decode)]
struct RawPolkaSignature {
    address: RawAddress,
    signature: RawSignature,
}

#[derive(Encode, Decode)]
struct RawPolkaCertificate {
    height: u64,
    round: i64,
    value_id: [u8; 32],
    polka_signatures: Vec<RawPolkaSignature>,
}

#[derive(Encode, Decode)]
enum RawNilOrValValueId {
    Nil,
    Val([u8; 32]),
}

impl From<NilOrVal<ValueId>> for RawNilOrValValueId {
    fn from(v: NilOrVal<ValueId>) -> Self {
        match v {
            NilOrVal::Nil => Self::Nil,
            NilOrVal::Val(id) => Self::Val(id.0),
        }
    }
}

impl From<RawNilOrValValueId> for NilOrVal<ValueId> {
    fn from(v: RawNilOrValValueId) -> Self {
        match v {
            RawNilOrValValueId::Nil => NilOrVal::Nil,
            RawNilOrValValueId::Val(b) => NilOrVal::Val(ValueId(b)),
        }
    }
}

#[derive(Encode, Decode)]
struct RawRoundSignature {
    vote_type: u8,
    value_id: RawNilOrValValueId,
    address: RawAddress,
    signature: RawSignature,
}

#[derive(Encode, Decode)]
struct RawRoundCertificate {
    height: u64,
    round: i64,
    cert_type: u8,
    round_signatures: Vec<RawRoundSignature>,
}

#[derive(Encode, Decode)]
enum RawLivenessMsg {
    Vote(RawSignedMessage),
    PolkaCertificate(RawPolkaCertificate),
    SkipRoundCertificate(RawRoundCertificate),
}

impl From<LivenessMsg<MalachiteCtx>> for RawLivenessMsg {
    fn from(value: LivenessMsg<MalachiteCtx>) -> Self {
        match value {
            LivenessMsg::Vote(vote) => Self::Vote(RawSignedMessage {
                message: vote.message.to_sign_bytes().to_vec(),
                signature: RawSignature::from(&vote.signature),
            }),
            LivenessMsg::PolkaCertificate(polka) => Self::PolkaCertificate(RawPolkaCertificate {
                height: polka.height.as_u64(),
                round: round_to_i64(polka.round),
                value_id: polka.value_id.0,
                polka_signatures: polka
                    .polka_signatures
                    .iter()
                    .map(|sig| RawPolkaSignature {
                        address: RawAddress::from(&sig.address),
                        signature: RawSignature::from(&sig.signature),
                    })
                    .collect(),
            }),
            LivenessMsg::SkipRoundCertificate(rc) => {
                Self::SkipRoundCertificate(RawRoundCertificate {
                    height: rc.height.as_u64(),
                    round: round_to_i64(rc.round),
                    cert_type: round_cert_type_to_u8(rc.cert_type),
                    round_signatures: rc
                        .round_signatures
                        .into_iter()
                        .map(|sig| RawRoundSignature {
                            vote_type: vote_type_to_u8(sig.vote_type),
                            value_id: RawNilOrValValueId::from(sig.value_id),
                            address: RawAddress::from(&sig.address),
                            signature: RawSignature::from(&sig.signature),
                        })
                        .collect(),
                })
            }
        }
    }
}

impl TryFrom<RawLivenessMsg> for LivenessMsg<MalachiteCtx> {
    type Error = CodecError;
    fn try_from(value: RawLivenessMsg) -> Result<Self, Self::Error> {
        Ok(match value {
            RawLivenessMsg::Vote(raw) => LivenessMsg::Vote(SignedVote {
                message: Vote::from_sign_bytes(&raw.message)?,
                signature: Signature::try_from(raw.signature)?,
            }),
            RawLivenessMsg::PolkaCertificate(cert) => {
                let mut polka_signatures = Vec::with_capacity(cert.polka_signatures.len());
                for s in cert.polka_signatures {
                    polka_signatures.push(PolkaSignature {
                        address: Address::from(s.address),
                        signature: Signature::try_from(s.signature)?,
                    });
                }
                LivenessMsg::PolkaCertificate(PolkaCertificate {
                    height: Height::new(cert.height),
                    round: i64_to_round(cert.round)?,
                    value_id: ValueId(cert.value_id),
                    polka_signatures,
                })
            }
            RawLivenessMsg::SkipRoundCertificate(cert) => {
                let mut round_signatures = Vec::with_capacity(cert.round_signatures.len());
                for s in cert.round_signatures {
                    round_signatures.push(RoundSignature {
                        vote_type: u8_to_vote_type(s.vote_type)?,
                        value_id: NilOrVal::from(s.value_id),
                        address: Address::from(s.address),
                        signature: Signature::try_from(s.signature)?,
                    });
                }
                LivenessMsg::SkipRoundCertificate(RoundCertificate {
                    height: Height::new(cert.height),
                    round: i64_to_round(cert.round)?,
                    cert_type: u8_to_round_cert_type(cert.cert_type)?,
                    round_signatures,
                })
            }
        })
    }
}

#[derive(Encode, Decode)]
struct RawProposedValue {
    height: u64,
    round: i64,
    valid_round: i64,
    proposer: RawAddress,
    value: Value,
    validity: bool,
}

impl From<ProposedValue<MalachiteCtx>> for RawProposedValue {
    fn from(p: ProposedValue<MalachiteCtx>) -> Self {
        Self {
            height: p.height.as_u64(),
            round: round_to_i64(p.round),
            valid_round: round_to_i64(p.valid_round),
            proposer: RawAddress::from(&p.proposer),
            value: p.value,
            validity: matches!(p.validity, Validity::Valid),
        }
    }
}

impl TryFrom<RawProposedValue> for ProposedValue<MalachiteCtx> {
    type Error = CodecError;
    fn try_from(p: RawProposedValue) -> Result<Self, Self::Error> {
        Ok(Self {
            height: Height::new(p.height),
            round: i64_to_round(p.round)?,
            valid_round: i64_to_round(p.valid_round)?,
            proposer: Address::from(p.proposer),
            value: p.value,
            validity: if p.validity {
                Validity::Valid
            } else {
                Validity::Invalid
            },
        })
    }
}

#[derive(Encode, Decode)]
struct RawValidatorProof {
    public_key: Vec<u8>,
    peer_id: Vec<u8>,
    signature: RawSignature,
}

impl From<ValidatorProof<MalachiteCtx>> for RawValidatorProof {
    fn from(value: ValidatorProof<MalachiteCtx>) -> Self {
        Self {
            public_key: value.public_key,
            peer_id: value.peer_id,
            signature: RawSignature::from(&value.signature),
        }
    }
}

impl TryFrom<RawValidatorProof> for ValidatorProof<MalachiteCtx> {
    type Error = CodecError;
    fn try_from(value: RawValidatorProof) -> Result<Self, Self::Error> {
        Ok(ValidatorProof::new(
            value.public_key,
            value.peer_id,
            Signature::try_from(value.signature)?,
        ))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn round_to_i64(r: Round) -> i64 {
    r.as_i64()
}

fn i64_to_round(v: i64) -> Result<Round, CodecError> {
    if v == -1 {
        Ok(Round::Nil)
    } else if v >= 0 && v <= u32::MAX as i64 {
        Ok(Round::new(v as u32))
    } else {
        Err(CodecError::from("Round out of range"))
    }
}

fn vote_type_to_u8(t: VoteType) -> u8 {
    match t {
        VoteType::Prevote => 0,
        VoteType::Precommit => 1,
    }
}

fn u8_to_vote_type(b: u8) -> Result<VoteType, CodecError> {
    match b {
        0 => Ok(VoteType::Prevote),
        1 => Ok(VoteType::Precommit),
        _ => Err(CodecError::from("invalid VoteType tag")),
    }
}

fn round_cert_type_to_u8(t: RoundCertificateType) -> u8 {
    match t {
        RoundCertificateType::Skip => 0,
        RoundCertificateType::Precommit => 1,
    }
}

fn u8_to_round_cert_type(b: u8) -> Result<RoundCertificateType, CodecError> {
    match b {
        0 => Ok(RoundCertificateType::Skip),
        1 => Ok(RoundCertificateType::Precommit),
        _ => Err(CodecError::from("invalid RoundCertificateType tag")),
    }
}

pub fn encode_value(value: &Value) -> Bytes {
    Bytes::from(Encode::encode(value))
}

pub fn decode_value(bytes: Bytes) -> Option<Value> {
    Value::decode(&mut &bytes[..]).ok()
}

pub fn encode_proposed_value(v: &ProposedValue<MalachiteCtx>) -> Vec<u8> {
    Encode::encode(&RawProposedValue::from(v.clone()))
}

pub fn decode_proposed_value(bytes: &[u8]) -> Result<ProposedValue<MalachiteCtx>, CodecError> {
    RawProposedValue::decode(&mut &bytes[..])?.try_into()
}

pub fn encode_commit_certificate(c: &CommitCertificate<MalachiteCtx>) -> Vec<u8> {
    Encode::encode(&RawCommitCertificate::from(c.clone()))
}

pub fn decode_commit_certificate(
    bytes: &[u8],
) -> Result<CommitCertificate<MalachiteCtx>, CodecError> {
    let raw = RawCommitCertificate::decode(&mut &bytes[..])?;
    CommitCertificate::try_from(raw)
}

pub fn encode_proposal_parts(parts: &crate::streaming::ProposalParts) -> Vec<u8> {
    Encode::encode(parts)
}

pub fn decode_proposal_parts(bytes: &[u8]) -> Result<crate::streaming::ProposalParts, CodecError> {
    crate::streaming::ProposalParts::decode(&mut &bytes[..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing::{MalachiteSigner, private_key_from_bytes};
    use proptest::prelude::*;

    #[test]
    fn value_round_trip() {
        let v = Value::new(b"hello".to_vec());
        let bytes = encode_value(&v);
        let back = decode_value(bytes).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn liveness_polka_cert_round_trip_preserves_signatures() {
        let mut bytes = [0u8; 32];
        bytes[31] = 7;
        let signer = MalachiteSigner::new(private_key_from_bytes(&bytes).unwrap());
        let pk = signer.public_key();
        let address = Address::from_public_key(&pk);
        let sig = signer.sign(b"sample");
        let msg = LivenessMsg::PolkaCertificate(PolkaCertificate {
            height: Height::new(7),
            round: Round::new(1),
            value_id: ValueId([0x42; 32]),
            polka_signatures: vec![PolkaSignature {
                address,
                signature: sig,
            }],
        });
        let codec = ScaleCodec;
        let encoded =
            <ScaleCodec as Codec<LivenessMsg<MalachiteCtx>>>::encode(&codec, &msg).expect("encode");
        let back = <ScaleCodec as Codec<LivenessMsg<MalachiteCtx>>>::decode(&codec, encoded)
            .expect("decode");
        match (msg, back) {
            (LivenessMsg::PolkaCertificate(orig), LivenessMsg::PolkaCertificate(back)) => {
                assert_eq!(orig.height, back.height);
                assert_eq!(orig.round, back.round);
                assert_eq!(orig.value_id, back.value_id);
                assert_eq!(orig.polka_signatures.len(), back.polka_signatures.len());
                assert_eq!(
                    orig.polka_signatures[0].address,
                    back.polka_signatures[0].address
                );
            }
            _ => panic!("variant mismatch"),
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn prop_value_round_trip(block in proptest::collection::vec(any::<u8>(), 0..256)) {
            let v = Value::new(block);
            let bytes = encode_value(&v);
            let back = decode_value(bytes).unwrap();
            prop_assert_eq!(v, back);
        }
    }
}
