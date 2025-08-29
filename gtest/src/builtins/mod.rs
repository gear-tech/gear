mod bls12_381;
mod eth_bridge;

pub use bls12_381::{
    BLS12_381_ID, Request as Bls12_381Request, Response as Bls12_381Response,
    process_bls12_381_dispatch,
};
pub use eth_bridge::{ETH_BRIDGE_ID, EthBridgeRequest, EthBridgeResponse};
pub(crate) use eth_bridge::{create_bridge_call_output, process_eth_bridge_dispatch};

use core_processor::common::{ActorExecutionErrorReplyReason, TrapExplanation};
use gear_core::str::LimitedStr;
use parity_scale_codec::{Decode, Encode};

#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq)]
pub enum BuiltinActorError {
    /// Occurs if the underlying call has the weight greater than the `gas_limit`.
    InsufficientGas,
    /// Occurs if the dispatch's value is less than the minimum required value.
    InsufficientValue,
    /// Occurs if the dispatch's message can't be decoded into a known type.
    DecodingError,
    /// Actor's inner error encoded as a String.
    Custom(LimitedStr<'static>),
    /// Occurs if a builtin actor execution does not fit in the current block.
    GasAllowanceExceeded,
    /// The array of G1-points is empty.
    EmptyPointList,
    /// Failed to create `MapToCurveBasedHasher`.
    MapperCreationError,
    /// Failed to map a message to a G2-point.
    MessageMappingError,
}

impl From<BuiltinActorError> for ActorExecutionErrorReplyReason {
    /// Convert [`BuiltinActorError`] to [`core_processor::common::ActorExecutionErrorReplyReason`]
    fn from(err: BuiltinActorError) -> Self {
        match err {
            BuiltinActorError::InsufficientGas => {
                ActorExecutionErrorReplyReason::Trap(TrapExplanation::GasLimitExceeded)
            }
            BuiltinActorError::InsufficientValue => {
                ActorExecutionErrorReplyReason::Trap(TrapExplanation::Panic(
                    LimitedStr::from_small_str("Not enough value supplied").into(),
                ))
            }
            BuiltinActorError::DecodingError => ActorExecutionErrorReplyReason::Trap(
                TrapExplanation::Panic(LimitedStr::from_small_str("Message decoding error").into()),
            ),
            BuiltinActorError::Custom(e) => {
                ActorExecutionErrorReplyReason::Trap(TrapExplanation::Panic(e.into()))
            }
            BuiltinActorError::EmptyPointList => {
                ActorExecutionErrorReplyReason::Trap(TrapExplanation::Panic(
                    LimitedStr::from_small_str("The array of G1-points is empty").into(),
                ))
            }
            BuiltinActorError::MapperCreationError => {
                ActorExecutionErrorReplyReason::Trap(TrapExplanation::Panic(
                    LimitedStr::from_small_str("Failed to create `MapToCurveBasedHasher`").into(),
                ))
            }
            BuiltinActorError::MessageMappingError => {
                ActorExecutionErrorReplyReason::Trap(TrapExplanation::Panic(
                    LimitedStr::from_small_str("Failed to map a message to a G2-point").into(),
                ))
            }
            BuiltinActorError::GasAllowanceExceeded => {
                unreachable!("Never supposed to be converted to error reply reason")
            }
        }
    }
}
