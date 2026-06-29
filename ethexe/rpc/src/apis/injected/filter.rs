// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_common::injected::Promise;
use gear_core::message::ReplyCode;
use serde::{Deserialize, Serialize};

/// Server-side filter for `subscribe_promises`, applied per subscriber so each
/// client receives only the promises it asked for. Omitted / `None` streams
/// every promise.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromiseSubscriptionFilter {
    /// Predicate on `promise.reply.code`. Named `reply_code` (not `code`) to
    /// avoid confusion with the program code ids used by the neighbouring
    /// `code` RPC API.
    #[serde(default)]
    pub reply_code: Option<ReplyCodeFilter>,
}

impl PromiseSubscriptionFilter {
    /// Returns `true` if `promise` passes all predicates in this filter.
    pub(crate) fn matches(&self, promise: &Promise) -> bool {
        self.reply_code
            .as_ref()
            .is_none_or(|f| f.matches(&promise.reply.code))
    }
}

/// Reply-code predicate. `Success` / `Error` match the coarse `ReplyCode`
/// variant; `Exact` uses the canonical hex form that already appears in
/// `promise.reply.code` on the wire: lowercase `"0x"` prefix followed by
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
    use gear_core::{
        message::{ReplyCode, SuccessReplyReason},
        rpc::ReplyInfo,
    };

    fn success_promise() -> Promise {
        Promise {
            tx_hash: Default::default(),
            reply: ReplyInfo {
                payload: vec![],
                value: 0,
                code: ReplyCode::Success(SuccessReplyReason::Manual),
            },
        }
    }

    fn error_promise() -> Promise {
        Promise {
            tx_hash: Default::default(),
            reply: ReplyInfo {
                payload: vec![],
                value: 0,
                code: ReplyCode::Unsupported,
            },
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
        let filter = PromiseSubscriptionFilter {
            reply_code: Some(ReplyCodeFilter::Exact {
                code: ReplyCode::Success(SuccessReplyReason::Manual),
            }),
        };
        assert!(filter.matches(&success_promise()));
        assert!(!filter.matches(&error_promise()));
    }
}
