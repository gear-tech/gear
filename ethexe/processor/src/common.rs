use ethexe_common::router::StateTransition;
use gprimitives::CodeId;
use parity_scale_codec::{Decode, Encode};

/// Local changes that can be committed to the network or local signer.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub enum LocalOutcome {
    /// Produced when code with specific id is recorded and validated.
    CodeValidated {
        id: CodeId,
        valid: bool,
    },

    Transition(StateTransition),
}
