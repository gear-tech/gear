use codec::MaxEncodedLen;

use super::*;

#[derive(Clone, Decode, Debug, Encode, MaxEncodedLen, TypeInfo)]
pub enum ValueType<ExternalId, Id, Balance> {
    External { id: ExternalId, value: Balance },
    ReservedLocal { id: ExternalId, value: Balance },
    SpecifiedLocal { parent: Id, value: Balance },
    UnspecifiedLocal { parent: Id },
}

impl<ExternalId: Default, Id, Balance: Zero> Default for ValueType<ExternalId, Id, Balance> {
    fn default() -> Self {
        ValueType::External {
            id: Default::default(),
            value: Zero::zero(),
        }
    }
}

#[derive(Clone, Default, Decode, Debug, Encode, MaxEncodedLen, TypeInfo)]
pub struct ValueNode<ExternalId: Default + Clone, Id: Clone, Balance: Zero + Clone> {
    pub spec_refs: u32,
    pub unspec_refs: u32,
    pub inner: ValueType<ExternalId, Id, Balance>,
    pub consumed: bool,
}

impl<ExternalId: Default + Clone, Id: Clone + Copy, Balance: Zero + Clone + Copy>
    ValueNode<ExternalId, Id, Balance>
{
    pub fn new(origin: ExternalId, value: Balance) -> Self {
        Self {
            inner: ValueType::External { id: origin, value },
            spec_refs: 0,
            unspec_refs: 0,
            consumed: false,
        }
    }

    pub fn inner_value(&self) -> Option<Balance> {
        match self.inner {
            ValueType::External { value, .. } => Some(value),
            ValueType::ReservedLocal { value, .. } => Some(value),
            ValueType::SpecifiedLocal { value, .. } => Some(value),
            ValueType::UnspecifiedLocal { .. } => None,
        }
    }

    pub fn inner_value_mut(&mut self) -> Option<&mut Balance> {
        match self.inner {
            ValueType::External { ref mut value, .. } => Some(value),
            ValueType::ReservedLocal { ref mut value, .. } => Some(value),
            ValueType::SpecifiedLocal { ref mut value, .. } => Some(value),
            ValueType::UnspecifiedLocal { .. } => None,
        }
    }

    pub fn parent(&self) -> Option<Id> {
        match self.inner {
            ValueType::External { .. } | ValueType::ReservedLocal { .. } => None,
            ValueType::SpecifiedLocal { parent, .. } | ValueType::UnspecifiedLocal { parent } => {
                Some(parent)
            }
        }
    }

    pub fn refs(&self) -> u32 {
        self.spec_refs.saturating_add(self.unspec_refs)
    }
}
