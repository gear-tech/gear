use crate::{Call, Calls};
use alloc::vec::Vec;
use parity_scale_codec::{Decode, Encode};

#[derive(Encode, Decode, Clone, Debug)]
/// Represents behavior pattern of `demo_constructor`.
/// This type will be parsed as init payload of `demo_constructor`.
pub enum Scheme {
    /// Direct scheme forces program execute commands from incoming payload.
    /// Inner argument is calls to be executed inside init function.
    ///
    /// Better to use this scheme for really easy demos the only
    /// interacts with user.
    Direct(Vec<Call>),
    /// Predefined scheme forces program execute the same commands each execution.
    /// Inner arguments are calls to be executed inside
    /// (init, handle, handle_reply) functions.
    ///
    /// Better to use this scheme if you need common-purpose program that
    /// executes the same commands across different incoming payloads.
    Predefined(Vec<Call>, Vec<Call>, Vec<Call>),
}

impl Scheme {
    pub fn empty() -> Self {
        Self::Direct(Default::default())
    }

    pub fn direct(init: Calls) -> Self {
        Self::Direct(init.calls())
    }

    pub fn predefined(init: Calls, handle: Calls, handle_reply: Calls) -> Self {
        Self::Predefined(init.calls(), handle.calls(), handle_reply.calls())
    }

    pub fn init(&self) -> &Vec<Call> {
        match self {
            Self::Direct(init) => init,
            Self::Predefined(init, ..) => init,
        }
    }

    pub fn handle(&self) -> Option<&Vec<Call>> {
        match self {
            Self::Predefined(_, handle, _) => Some(handle),
            _ => None,
        }
    }

    pub fn handle_reply(&self) -> Option<&Vec<Call>> {
        match self {
            Self::Predefined(_, _, handle_reply) => Some(handle_reply),
            _ => None,
        }
    }
}
