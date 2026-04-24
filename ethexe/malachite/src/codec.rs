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

//! JSON-based codec for the Malachite engine.
//!
//! Malachite's internal types are generic over `Context` and don't
//! derive `Serialize`/`Deserialize` directly. The pattern used upstream
//! is to declare local `Raw*` wrapper types that DO derive serde and
//! define `From` conversions. We follow that pattern here, adapted to
//! our [`EthexeContext`].

use bytes::Bytes;
use ed25519_consensus::Signature;
use serde::{Deserialize, Serialize};

use malachitebft_app::streaming::StreamId;
use malachitebft_codec::{Codec, HasEncodedLen};
use malachitebft_core_consensus::{LivenessMsg, SignedConsensusMsg};
use malachitebft_core_types::{
    CommitCertificate, CommitSignature, NilOrVal, PolkaCertificate, PolkaSignature, Round,
    RoundCertificate, RoundCertificateType, RoundSignature, SignedProposal, SignedVote,
    ValidatorProof, VoteType,
};
use malachitebft_engine::util::streaming::{StreamContent, StreamMessage};
use malachitebft_sync::{
    PeerId, RawDecidedValue, Request, Response, Status, ValueRequest, ValueResponse,
};

use crate::context::{Address, EthexeContext, Height, Proposal, ProposalPart, ValueId, Value, Vote};

// ---------------------------------------------------------------------------
// JsonCodec — top-level type implementing `Codec<T>` for each type the
// engine asks us to encode/decode.
// ---------------------------------------------------------------------------

#[derive(Copy, Clone, Debug, Default)]
pub struct JsonCodec;

impl Codec<Value> for JsonCodec {
    type Error = serde_json::Error;
    fn decode(&self, bytes: Bytes) -> Result<Value, Self::Error> {
        serde_json::from_slice(&bytes)
    }
    fn encode(&self, msg: &Value) -> Result<Bytes, Self::Error> {
        serde_json::to_vec(msg).map(Bytes::from)
    }
}

impl Codec<ProposalPart> for JsonCodec {
    type Error = serde_json::Error;
    fn decode(&self, bytes: Bytes) -> Result<ProposalPart, Self::Error> {
        serde_json::from_slice(&bytes)
    }
    fn encode(&self, msg: &ProposalPart) -> Result<Bytes, Self::Error> {
        serde_json::to_vec(msg).map(Bytes::from)
    }
}

impl Codec<SignedConsensusMsg<EthexeContext>> for JsonCodec {
    type Error = serde_json::Error;
    fn decode(&self, bytes: Bytes) -> Result<SignedConsensusMsg<EthexeContext>, Self::Error> {
        serde_json::from_slice::<RawSignedConsensusMsg>(&bytes).map(Into::into)
    }
    fn encode(&self, msg: &SignedConsensusMsg<EthexeContext>) -> Result<Bytes, Self::Error> {
        serde_json::to_vec(&RawSignedConsensusMsg::from(msg.clone())).map(Bytes::from)
    }
}

impl Codec<StreamMessage<ProposalPart>> for JsonCodec {
    type Error = serde_json::Error;
    fn decode(&self, bytes: Bytes) -> Result<StreamMessage<ProposalPart>, Self::Error> {
        serde_json::from_slice::<RawStreamMessage>(&bytes).map(Into::into)
    }
    fn encode(&self, msg: &StreamMessage<ProposalPart>) -> Result<Bytes, Self::Error> {
        serde_json::to_vec(&RawStreamMessage::from(msg.clone())).map(Bytes::from)
    }
}

impl Codec<Status<EthexeContext>> for JsonCodec {
    type Error = serde_json::Error;
    fn decode(&self, bytes: Bytes) -> Result<Status<EthexeContext>, Self::Error> {
        serde_json::from_slice::<RawStatus>(&bytes).map(Into::into)
    }
    fn encode(&self, msg: &Status<EthexeContext>) -> Result<Bytes, Self::Error> {
        serde_json::to_vec(&RawStatus::from(msg.clone())).map(Bytes::from)
    }
}

impl Codec<Request<EthexeContext>> for JsonCodec {
    type Error = serde_json::Error;
    fn decode(&self, bytes: Bytes) -> Result<Request<EthexeContext>, Self::Error> {
        serde_json::from_slice::<RawRequest>(&bytes).map(Into::into)
    }
    fn encode(&self, msg: &Request<EthexeContext>) -> Result<Bytes, Self::Error> {
        serde_json::to_vec(&RawRequest::from(msg.clone())).map(Bytes::from)
    }
}

impl Codec<Response<EthexeContext>> for JsonCodec {
    type Error = serde_json::Error;
    fn decode(&self, bytes: Bytes) -> Result<Response<EthexeContext>, Self::Error> {
        serde_json::from_slice::<RawResponse>(&bytes).map(Into::into)
    }
    fn encode(&self, msg: &Response<EthexeContext>) -> Result<Bytes, Self::Error> {
        serde_json::to_vec(&RawResponse::from(msg.clone())).map(Bytes::from)
    }
}

impl HasEncodedLen<Response<EthexeContext>> for JsonCodec {
    fn encoded_len(
        &self,
        msg: &Response<EthexeContext>,
    ) -> Result<usize, <Self as Codec<Response<EthexeContext>>>::Error> {
        // Serialize to measure — acceptable for JSON since length is
        // not pre-computable.
        serde_json::to_vec(&RawResponse::from(msg.clone())).map(|b| b.len())
    }
}

impl Codec<LivenessMsg<EthexeContext>> for JsonCodec {
    type Error = serde_json::Error;
    fn decode(&self, bytes: Bytes) -> Result<LivenessMsg<EthexeContext>, Self::Error> {
        serde_json::from_slice::<RawLivenessMsg>(&bytes).map(Into::into)
    }
    fn encode(&self, msg: &LivenessMsg<EthexeContext>) -> Result<Bytes, Self::Error> {
        serde_json::to_vec(&RawLivenessMsg::from(msg.clone())).map(Bytes::from)
    }
}

impl Codec<ValidatorProof<EthexeContext>> for JsonCodec {
    type Error = serde_json::Error;
    fn decode(&self, bytes: Bytes) -> Result<ValidatorProof<EthexeContext>, Self::Error> {
        serde_json::from_slice::<RawValidatorProof>(&bytes).map(Into::into)
    }
    fn encode(&self, msg: &ValidatorProof<EthexeContext>) -> Result<Bytes, Self::Error> {
        serde_json::to_vec(&RawValidatorProof::from(msg.clone())).map(Bytes::from)
    }
}

impl Codec<malachitebft_core_consensus::ProposedValue<EthexeContext>> for JsonCodec {
    type Error = serde_json::Error;
    fn decode(
        &self,
        bytes: Bytes,
    ) -> Result<malachitebft_core_consensus::ProposedValue<EthexeContext>, Self::Error> {
        serde_json::from_slice::<RawProposedValue>(&bytes).map(Into::into)
    }
    fn encode(
        &self,
        msg: &malachitebft_core_consensus::ProposedValue<EthexeContext>,
    ) -> Result<Bytes, Self::Error> {
        serde_json::to_vec(&RawProposedValue::from(msg.clone())).map(Bytes::from)
    }
}

impl Codec<CommitCertificate<EthexeContext>> for JsonCodec {
    type Error = serde_json::Error;
    fn decode(&self, bytes: Bytes) -> Result<CommitCertificate<EthexeContext>, Self::Error> {
        serde_json::from_slice::<RawCommitCertificate>(&bytes).map(Into::into)
    }
    fn encode(&self, msg: &CommitCertificate<EthexeContext>) -> Result<Bytes, Self::Error> {
        serde_json::to_vec(&RawCommitCertificate::from(msg.clone())).map(Bytes::from)
    }
}

// ---------------------------------------------------------------------------
// Raw wrapper types (owned by us so serde derives work)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct RawSignedMessage {
    message: Bytes,
    signature: Signature,
}

#[derive(Serialize, Deserialize)]
enum RawSignedConsensusMsg {
    Vote(RawSignedMessage),
    Proposal(RawSignedMessage),
}

impl From<SignedConsensusMsg<EthexeContext>> for RawSignedConsensusMsg {
    fn from(value: SignedConsensusMsg<EthexeContext>) -> Self {
        match value {
            SignedConsensusMsg::Vote(vote) => Self::Vote(RawSignedMessage {
                message: vote.message.to_sign_bytes(),
                signature: *vote.signature.inner(),
            }),
            SignedConsensusMsg::Proposal(proposal) => Self::Proposal(RawSignedMessage {
                message: proposal.message.to_sign_bytes(),
                signature: *proposal.signature.inner(),
            }),
        }
    }
}

impl From<RawSignedConsensusMsg> for SignedConsensusMsg<EthexeContext> {
    fn from(value: RawSignedConsensusMsg) -> Self {
        match value {
            RawSignedConsensusMsg::Vote(raw) => SignedConsensusMsg::Vote(SignedVote {
                message: Vote::from_sign_bytes(&raw.message).unwrap(),
                signature: raw.signature.into(),
            }),
            RawSignedConsensusMsg::Proposal(raw) => SignedConsensusMsg::Proposal(SignedProposal {
                message: Proposal::from_sign_bytes(&raw.message).unwrap(),
                signature: raw.signature.into(),
            }),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "StreamId")]
struct RawStreamId(#[serde(getter = "StreamId::to_bytes")] Bytes);

impl From<RawStreamId> for StreamId {
    fn from(value: RawStreamId) -> Self {
        Self::new(value.0)
    }
}

#[derive(Serialize, Deserialize)]
struct RawStreamMessage {
    #[serde(with = "RawStreamId")]
    stream_id: StreamId,
    sequence: u64,
    content: RawStreamContent,
}

#[derive(Serialize, Deserialize)]
enum RawStreamContent {
    Data(ProposalPart),
    Fin,
}

impl From<StreamMessage<ProposalPart>> for RawStreamMessage {
    fn from(value: StreamMessage<ProposalPart>) -> Self {
        Self {
            stream_id: value.stream_id,
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
            stream_id: value.stream_id,
            sequence: value.sequence,
            content: match value.content {
                RawStreamContent::Data(part) => StreamContent::Data(part),
                RawStreamContent::Fin => StreamContent::Fin,
            },
        }
    }
}

#[derive(Serialize, Deserialize)]
struct RawStatus {
    peer_id: PeerId,
    tip_height: Height,
    history_min_height: Height,
}

impl From<Status<EthexeContext>> for RawStatus {
    fn from(value: Status<EthexeContext>) -> Self {
        Self {
            peer_id: value.peer_id,
            tip_height: value.tip_height,
            history_min_height: value.history_min_height,
        }
    }
}

impl From<RawStatus> for Status<EthexeContext> {
    fn from(value: RawStatus) -> Self {
        Self {
            peer_id: value.peer_id,
            tip_height: value.tip_height,
            history_min_height: value.history_min_height,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct ValueRawRequest {
    height: Height,
    end_height: Option<Height>,
}

#[derive(Serialize, Deserialize)]
enum RawRequest {
    SyncRequest(ValueRawRequest),
}

impl From<Request<EthexeContext>> for RawRequest {
    fn from(value: Request<EthexeContext>) -> Self {
        match value {
            Request::ValueRequest(request) => Self::SyncRequest(ValueRawRequest {
                height: *request.range.start(),
                end_height: Some(*request.range.end()),
            }),
        }
    }
}

impl From<RawRequest> for Request<EthexeContext> {
    fn from(value: RawRequest) -> Self {
        match value {
            RawRequest::SyncRequest(raw) => Self::ValueRequest(ValueRequest {
                range: raw.height..=raw.end_height.unwrap_or(raw.height),
            }),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct RawCommitSignature {
    address: Address,
    signature: Signature,
}

#[derive(Serialize, Deserialize)]
struct RawCommitSignatures {
    signatures: Vec<RawCommitSignature>,
}

#[derive(Serialize, Deserialize)]
struct RawCommitCertificate {
    height: Height,
    round: Round,
    value_id: ValueId,
    commit_signatures: RawCommitSignatures,
}

impl From<RawCommitCertificate> for CommitCertificate<EthexeContext> {
    fn from(value: RawCommitCertificate) -> Self {
        CommitCertificate {
            height: value.height,
            round: value.round,
            value_id: value.value_id,
            commit_signatures: value
                .commit_signatures
                .signatures
                .into_iter()
                .map(|sig| CommitSignature {
                    address: sig.address,
                    signature: sig.signature.into(),
                })
                .collect(),
        }
    }
}

impl From<CommitCertificate<EthexeContext>> for RawCommitCertificate {
    fn from(value: CommitCertificate<EthexeContext>) -> Self {
        Self {
            height: value.height,
            round: value.round,
            value_id: value.value_id,
            commit_signatures: RawCommitSignatures {
                signatures: value
                    .commit_signatures
                    .iter()
                    .map(|sig| RawCommitSignature {
                        address: sig.address,
                        signature: *sig.signature.inner(),
                    })
                    .collect(),
            },
        }
    }
}

#[derive(Serialize, Deserialize)]
struct RawSyncedValue {
    value_bytes: Bytes,
    certificate: RawCommitCertificate,
}

#[derive(Serialize, Deserialize)]
struct ValueRawResponse {
    start_height: Height,
    value: Vec<RawSyncedValue>,
}

impl From<ValueResponse<EthexeContext>> for ValueRawResponse {
    fn from(response: ValueResponse<EthexeContext>) -> Self {
        Self {
            start_height: response.start_height,
            value: response
                .values
                .into_iter()
                .map(|v| RawSyncedValue {
                    value_bytes: v.value_bytes,
                    certificate: v.certificate.into(),
                })
                .collect(),
        }
    }
}

impl From<ValueRawResponse> for ValueResponse<EthexeContext> {
    fn from(response: ValueRawResponse) -> Self {
        Self {
            start_height: response.start_height,
            values: response
                .value
                .into_iter()
                .map(|v| RawDecidedValue {
                    value_bytes: v.value_bytes,
                    certificate: v.certificate.into(),
                })
                .collect(),
        }
    }
}

#[derive(Serialize, Deserialize)]
enum RawResponse {
    ValueResponse(ValueRawResponse),
}

impl From<Response<EthexeContext>> for RawResponse {
    fn from(value: Response<EthexeContext>) -> Self {
        match value {
            Response::ValueResponse(resp) => Self::ValueResponse(resp.into()),
        }
    }
}

impl From<RawResponse> for Response<EthexeContext> {
    fn from(value: RawResponse) -> Self {
        match value {
            RawResponse::ValueResponse(resp) => Self::ValueResponse(resp.into()),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct RawPolkaSignature {
    address: Address,
    signature: Signature,
}

#[derive(Serialize, Deserialize)]
struct RawPolkaCertificate {
    height: Height,
    round: Round,
    value_id: ValueId,
    polka_signatures: Vec<RawPolkaSignature>,
}

#[derive(Serialize, Deserialize)]
struct RawRoundSignature {
    vote_type: VoteType,
    value_id: NilOrVal<ValueId>,
    address: Address,
    signature: Signature,
}

#[derive(Serialize, Deserialize)]
struct RawRoundCertificate {
    height: Height,
    round: Round,
    cert_type: RoundCertificateType,
    round_signatures: Vec<RawRoundSignature>,
}

#[derive(Serialize, Deserialize)]
enum RawLivenessMsg {
    Vote(RawSignedMessage),
    PolkaCertificate(RawPolkaCertificate),
    SkipRoundCertificate(RawRoundCertificate),
}

impl From<LivenessMsg<EthexeContext>> for RawLivenessMsg {
    fn from(value: LivenessMsg<EthexeContext>) -> Self {
        match value {
            LivenessMsg::Vote(vote) => Self::Vote(RawSignedMessage {
                message: vote.message.to_sign_bytes(),
                signature: *vote.signature.inner(),
            }),
            LivenessMsg::PolkaCertificate(polka) => Self::PolkaCertificate(RawPolkaCertificate {
                height: polka.height,
                round: polka.round,
                value_id: polka.value_id,
                polka_signatures: vec![], // TODO populate from real polka_signatures field
            }),
            LivenessMsg::SkipRoundCertificate(rc) => {
                Self::SkipRoundCertificate(RawRoundCertificate {
                    height: rc.height,
                    round: rc.round,
                    cert_type: rc.cert_type,
                    round_signatures: rc
                        .round_signatures
                        .into_iter()
                        .map(|sig| RawRoundSignature {
                            vote_type: sig.vote_type,
                            value_id: sig.value_id,
                            address: sig.address,
                            signature: *sig.signature.inner(),
                        })
                        .collect(),
                })
            }
        }
    }
}

impl From<RawLivenessMsg> for LivenessMsg<EthexeContext> {
    fn from(value: RawLivenessMsg) -> Self {
        match value {
            RawLivenessMsg::Vote(raw) => LivenessMsg::Vote(SignedVote {
                message: Vote::from_sign_bytes(&raw.message).unwrap(),
                signature: raw.signature.into(),
            }),
            RawLivenessMsg::PolkaCertificate(cert) => {
                LivenessMsg::PolkaCertificate(PolkaCertificate {
                    height: cert.height,
                    round: cert.round,
                    value_id: cert.value_id,
                    polka_signatures: cert
                        .polka_signatures
                        .into_iter()
                        .map(|s| PolkaSignature {
                            address: s.address,
                            signature: s.signature.into(),
                        })
                        .collect(),
                })
            }
            RawLivenessMsg::SkipRoundCertificate(cert) => {
                LivenessMsg::SkipRoundCertificate(RoundCertificate {
                    height: cert.height,
                    round: cert.round,
                    cert_type: cert.cert_type,
                    round_signatures: cert
                        .round_signatures
                        .into_iter()
                        .map(|s| RoundSignature {
                            vote_type: s.vote_type,
                            value_id: s.value_id,
                            address: s.address,
                            signature: s.signature.into(),
                        })
                        .collect(),
                })
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
struct RawProposedValue {
    height: Height,
    round: Round,
    valid_round: Round,
    proposer: Address,
    value: Value,
    validity: bool,
}

impl From<malachitebft_core_consensus::ProposedValue<EthexeContext>> for RawProposedValue {
    fn from(p: malachitebft_core_consensus::ProposedValue<EthexeContext>) -> Self {
        Self {
            height: p.height,
            round: p.round,
            valid_round: p.valid_round,
            proposer: p.proposer,
            value: p.value,
            validity: matches!(p.validity, malachitebft_core_types::Validity::Valid),
        }
    }
}

impl From<RawProposedValue> for malachitebft_core_consensus::ProposedValue<EthexeContext> {
    fn from(p: RawProposedValue) -> Self {
        Self {
            height: p.height,
            round: p.round,
            valid_round: p.valid_round,
            proposer: p.proposer,
            value: p.value,
            validity: if p.validity {
                malachitebft_core_types::Validity::Valid
            } else {
                malachitebft_core_types::Validity::Invalid
            },
        }
    }
}

#[derive(Serialize, Deserialize)]
struct RawValidatorProof {
    #[serde(with = "hex::serde")]
    public_key: Vec<u8>,
    #[serde(with = "hex::serde")]
    peer_id: Vec<u8>,
    signature: Signature,
}

impl From<ValidatorProof<EthexeContext>> for RawValidatorProof {
    fn from(value: ValidatorProof<EthexeContext>) -> Self {
        Self {
            public_key: value.public_key,
            peer_id: value.peer_id,
            signature: *value.signature.inner(),
        }
    }
}

impl From<RawValidatorProof> for ValidatorProof<EthexeContext> {
    fn from(value: RawValidatorProof) -> Self {
        ValidatorProof::new(value.public_key, value.peer_id, value.signature.into())
    }
}
