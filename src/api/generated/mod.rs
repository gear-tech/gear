mod metadata;

pub use metadata::*;

use gear_core::ids;
use metadata::api::runtime_types::gear_core::ids as generated_ids;

impl From<ids::MessageId> for generated_ids::MessageId {
    fn from(id: ids::MessageId) -> Self {
        Self(id.into())
    }
}

impl From<ids::ProgramId> for generated_ids::ProgramId {
    fn from(id: ids::ProgramId) -> Self {
        Self(id.into())
    }
}

impl From<ids::CodeId> for generated_ids::CodeId {
    fn from(id: ids::CodeId) -> Self {
        Self(id.into())
    }
}
