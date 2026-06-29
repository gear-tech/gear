// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_common::{Address, injected::Promise};
use gear_core::message::ReplyCode;
use gprimitives::ActorId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{collections::HashSet, hash::Hash};

/// A computed [`Promise`] enriched with routing information from its originating
/// injected transaction. This is the item type streamed by `subscribe_promises`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromiseEnvelope {
    /// Computed promise body.
    pub promise: Promise,
    /// Destination program of the originating injected transaction.
    pub destination: ActorId,
    /// Recovered signer of the originating injected transaction.
    ///
    /// Populated by looking up the stored `SignedInjectedTransaction` by `tx_hash`. If two
    /// different signers submitted identical transaction data (same destination, payload, value,
    /// reference block, and salt), the database may contain either signer. The canonical fix is
    /// for the compute layer to pass the winning signer alongside the [`Promise`] so no DB lookup
    /// is required.
    pub sender: Address,
}

/// A single value or a list of values used in subscription filter fields.
/// Serializes as a JSON scalar when there is one element and as a JSON array
/// when there are multiple, matching the convention used by Ethereum filter APIs.
///
/// Equivalent to `alloy_rpc_types_eth::ValueOrArray` but with generic `From<T>` and
/// `From<Vec<T>>` impls so it works with any item type, not only alloy primitives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ValueOrArray<T> {
    Value(T),
    Array(Vec<T>),
}

impl<T> From<T> for ValueOrArray<T> {
    fn from(v: T) -> Self {
        Self::Value(v)
    }
}

impl<T> From<Vec<T>> for ValueOrArray<T> {
    fn from(v: Vec<T>) -> Self {
        Self::Array(v)
    }
}

/// A set of values used as a subscription filter predicate.
/// An empty set matches every value (wildcard).
/// Serializes as a JSON scalar for a single element, array for multiple.
///
/// Equivalent to `alloy_rpc_types_eth::FilterSet` but with serde that accepts both a JSON
/// scalar and a JSON array (alloy's version only deserializes from an array).
#[derive(Debug, Clone, Default)]
pub struct FilterSet<T>(HashSet<T>);

impl<T: Eq + Hash> From<ValueOrArray<T>> for FilterSet<T> {
    fn from(voa: ValueOrArray<T>) -> Self {
        match voa {
            ValueOrArray::Value(v) => Self(std::iter::once(v).collect()),
            ValueOrArray::Array(vs) => Self(vs.into_iter().collect()),
        }
    }
}

impl<T: Eq + Hash> FilterSet<T> {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn matches(&self, value: &T) -> bool {
        self.0.is_empty() || self.0.contains(value)
    }

    fn to_value_or_array(&self) -> Option<ValueOrArray<&T>> {
        let mut iter = self.0.iter();
        match (iter.next(), iter.next()) {
            (None, _) => None,
            (Some(v), None) => Some(ValueOrArray::Value(v)),
            _ => Some(ValueOrArray::Array(self.0.iter().collect())),
        }
    }
}

impl<T: Serialize + Eq + Hash> Serialize for FilterSet<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.to_value_or_array() {
            Some(voa) => voa.serialize(s),
            None => s.serialize_none(),
        }
    }
}

impl<'de, T> Deserialize<'de> for FilterSet<T>
where
    T: serde::de::DeserializeOwned + Eq + Hash,
{
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // null => empty set (wildcard); scalar or array => populated set.
        Ok(match Option::<ValueOrArray<T>>::deserialize(d)? {
            Some(voa) => Self::from(voa),
            None => FilterSet(HashSet::new()),
        })
    }
}

/// Server-side filter for `subscribe_promises`, applied per subscriber so each
/// client receives only the promises it asked for. Omitted / `None` streams every
/// promise.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromiseSubscriptionFilter {
    #[serde(default, skip_serializing_if = "FilterSet::is_empty")]
    sender: FilterSet<Address>,
    #[serde(default, skip_serializing_if = "FilterSet::is_empty")]
    destination: FilterSet<ActorId>,
    /// Predicate on `promise.reply.code`. Named `reply_code` (not `code`) to
    /// avoid confusion with the program code ids used by the neighbouring
    /// `code` RPC API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_code: Option<ReplyCodeFilter>,
}

impl PromiseSubscriptionFilter {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn sender<T: Into<ValueOrArray<Address>>>(mut self, sender: T) -> Self {
        self.sender = FilterSet::from(sender.into());
        self
    }

    #[must_use]
    pub fn destination<T: Into<ValueOrArray<ActorId>>>(mut self, destination: T) -> Self {
        self.destination = FilterSet::from(destination.into());
        self
    }

    #[must_use]
    pub fn reply_code(mut self, reply_code: ReplyCodeFilter) -> Self {
        self.reply_code = Some(reply_code);
        self
    }

    /// Returns `true` if `envelope` passes all predicates in this filter.
    pub(crate) fn matches(&self, envelope: &PromiseEnvelope) -> bool {
        self.sender.matches(&envelope.sender)
            && self.destination.matches(&envelope.destination)
            && self
                .reply_code
                .as_ref()
                .is_none_or(|f| f.matches(&envelope.promise.reply.code))
    }
}

/// Reply-code predicate. `Success` / `Error` match the coarse `ReplyCode`
/// variant; `Exact` uses the canonical hex form: lowercase `"0x"` prefix followed by
/// exactly 8 hex digits — e.g. `"0x00010000"` = `Success(Manual)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ReplyCodeFilter {
    Success,
    Error,
    Exact {
        #[serde(with = "gear_core::rpc::serialize_reply_code")]
        code: ReplyCode,
    },
}

impl ReplyCodeFilter {
    pub const fn success() -> Self {
        Self::Success
    }

    pub const fn error() -> Self {
        Self::Error
    }

    pub const fn exact(code: ReplyCode) -> Self {
        Self::Exact { code }
    }

    /// Returns `true` if `code` satisfies this predicate.
    pub(crate) fn matches(&self, code: &ReplyCode) -> bool {
        match self {
            ReplyCodeFilter::Success => code.is_success(),
            ReplyCodeFilter::Error => code.is_error(),
            ReplyCodeFilter::Exact { code: expected } => expected == code,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gear_core::{message::SuccessReplyReason, rpc::ReplyInfo};

    fn envelope(sender: Address, destination: ActorId, code: ReplyCode) -> PromiseEnvelope {
        PromiseEnvelope {
            promise: Promise {
                tx_hash: Default::default(),
                reply: ReplyInfo {
                    payload: vec![],
                    value: 0,
                    code,
                },
            },
            destination,
            sender,
        }
    }

    #[test]
    fn exact_filter_deserializes_hex_code() {
        let filter = serde_json::from_value::<ReplyCodeFilter>(serde_json::json!({
            "type": "exact",
            "code": "0x00010000"
        }))
        .unwrap();

        assert!(matches!(
            filter,
            ReplyCodeFilter::Exact {
                code: ReplyCode::Success(SuccessReplyReason::Manual)
            }
        ));
    }

    #[test]
    fn exact_filter_rejects_malformed_code_during_deserialization() {
        let result = serde_json::from_value::<ReplyCodeFilter>(serde_json::json!({
            "type": "exact",
            "code": "not-a-code"
        }));

        assert!(result.is_err());
    }

    #[test]
    fn exact_filter_matches_correct_code() {
        let filter = PromiseSubscriptionFilter::new().reply_code(ReplyCodeFilter::exact(
            ReplyCode::Success(SuccessReplyReason::Manual),
        ));
        let dest = ActorId::from([0u8; 32]);
        assert!(filter.matches(&envelope(
            Address::from([0u8; 20]),
            dest,
            ReplyCode::Success(SuccessReplyReason::Manual),
        )));
        assert!(!filter.matches(&envelope(
            Address::from([0u8; 20]),
            dest,
            ReplyCode::Unsupported,
        )));
    }

    #[test]
    fn builder_matches_single_sender_and_destination() {
        let sender = Address::from([1u8; 20]);
        let destination = ActorId::from([2u8; 32]);
        let filter = PromiseSubscriptionFilter::new()
            .sender(sender)
            .destination(destination)
            .reply_code(ReplyCodeFilter::success());

        assert!(filter.matches(&envelope(
            sender,
            destination,
            ReplyCode::Success(SuccessReplyReason::Manual),
        )));
        assert!(!filter.matches(&envelope(
            Address::from([3u8; 20]),
            destination,
            ReplyCode::Success(SuccessReplyReason::Manual),
        )));
    }

    #[test]
    fn value_or_array_builders_accept_vectors_and_deduplicate() {
        let sender = Address::from([1u8; 20]);
        let destination = ActorId::from([2u8; 32]);
        let filter = PromiseSubscriptionFilter::new()
            .sender(vec![Address::from([3u8; 20]), sender, sender])
            .destination(vec![ActorId::from([4u8; 32]), destination, destination]);

        assert!(filter.matches(&envelope(sender, destination, ReplyCode::Unsupported)));
    }

    #[test]
    fn empty_set_is_a_wildcard() {
        let filter = PromiseSubscriptionFilter::new().sender(Vec::<Address>::new());
        assert!(filter.matches(&envelope(
            Address::from([1u8; 20]),
            ActorId::from([2u8; 32]),
            ReplyCode::Unsupported,
        )));
    }

    #[test]
    fn repeated_sender_call_replaces_the_previous_set() {
        let first = Address::from([1u8; 20]);
        let second = Address::from([2u8; 20]);
        let destination = ActorId::from([3u8; 32]);
        let filter = PromiseSubscriptionFilter::new()
            .sender(first)
            .sender(second);

        assert!(!filter.matches(&envelope(first, destination, ReplyCode::Unsupported)));
        assert!(filter.matches(&envelope(second, destination, ReplyCode::Unsupported)));
    }

    #[test]
    fn empty_filterset_round_trips_via_null() {
        let empty: FilterSet<Address> = FilterSet::default();

        let json = serde_json::to_value(&empty).expect("empty FilterSet must serialize");
        assert_eq!(
            json,
            serde_json::Value::Null,
            "empty FilterSet must serialize as null"
        );

        let back: FilterSet<Address> =
            serde_json::from_value(json).expect("null must deserialize back to empty FilterSet");
        assert!(back.is_empty());
        // Empty set is a wildcard — verify the semantic is preserved after the round-trip.
        assert!(back.matches(&Address::from([1u8; 20])));
    }

    #[test]
    fn value_or_array_serde_round_trip() {
        let first = Address::from([1u8; 20]);
        let second = Address::from([2u8; 20]);

        let scalar = serde_json::to_value(PromiseSubscriptionFilter::new().sender(first)).unwrap();
        assert_eq!(scalar["sender"], serde_json::to_value(first).unwrap());

        let array = serde_json::to_value(
            PromiseSubscriptionFilter::new().sender(vec![first, second, second]),
        )
        .unwrap();
        assert_eq!(array["sender"].as_array().unwrap().len(), 2);

        let scalar_filter: PromiseSubscriptionFilter = serde_json::from_value(scalar).unwrap();
        let array_filter: PromiseSubscriptionFilter = serde_json::from_value(array).unwrap();
        let destination = ActorId::from([3u8; 32]);
        assert!(scalar_filter.matches(&envelope(first, destination, ReplyCode::Unsupported)));
        assert!(array_filter.matches(&envelope(second, destination, ReplyCode::Unsupported)));
    }
}
