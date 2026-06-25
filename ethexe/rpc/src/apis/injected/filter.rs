// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_common::injected::Promise;
use gear_core::message::ReplyCode;
use serde::{Deserialize, Serialize};

/// Server-side filter for `subscribe_promises`, applied per subscriber so each
/// client receives only the promises it asked for. Omitted / `None` streams
/// every promise.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
/// variant; `Exact` matches the canonical hex form that already appears in
/// `promise.reply.code` on the wire: lowercase `"0x"` prefix followed by
/// exactly 8 hex digits — e.g. `"0x00010000"` = `Success(Manual)`. Any other
/// form (uppercase `"0X"`, fewer digits, non-hex characters) matches nothing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ReplyCodeFilter {
    Success,
    Error,
    Exact { code: String },
}

impl ReplyCodeFilter {
    /// Returns `true` if `code` satisfies this predicate.
    pub(crate) fn matches(&self, code: &ReplyCode) -> bool {
        match self {
            ReplyCodeFilter::Success => code.is_success(),
            ReplyCodeFilter::Error => code.is_error(),
            ReplyCodeFilter::Exact { code: hex } => {
                parse_hex_reply_code(hex).is_some_and(|expected| expected == *code)
            }
        }
    }
}

/// Parses the canonical `"0x"` + 8-hex-digit reply-code form into a
/// [`ReplyCode`]. Malformed input returns `None`, which matches nothing.
fn parse_hex_reply_code(code: &str) -> Option<ReplyCode> {
    let hex = code.strip_prefix("0x").unwrap_or(code);
    if hex.len() != 8 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut bytes = [0u8; 4];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(ReplyCode::from_bytes(bytes))
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
    fn parse_hex_reply_code_valid() {
        // "0x00010000" encodes Success(Manual) in little-endian bytes.
        let code = parse_hex_reply_code("0x00010000");
        assert_eq!(code, Some(ReplyCode::Success(SuccessReplyReason::Manual)));
    }

    #[test]
    fn parse_hex_reply_code_malformed_returns_none() {
        assert_eq!(parse_hex_reply_code(""), None);
        assert_eq!(parse_hex_reply_code("0x"), None);
        assert_eq!(parse_hex_reply_code("not-hex"), None);
        // Too short (6 hex digits instead of 8).
        assert_eq!(parse_hex_reply_code("0x000100"), None);
    }

    #[test]
    fn exact_filter_matches_correct_code() {
        let filter = PromiseSubscriptionFilter {
            reply_code: Some(ReplyCodeFilter::Exact {
                code: "0x00010000".to_string(),
            }),
        };
        assert!(filter.matches(&success_promise()));
        assert!(!filter.matches(&error_promise()));
    }

    #[test]
    fn exact_filter_malformed_code_matches_nothing() {
        let filter = PromiseSubscriptionFilter {
            reply_code: Some(ReplyCodeFilter::Exact {
                code: "not-a-code".to_string(),
            }),
        };
        // Malformed input must match nothing, not panic.
        assert!(!filter.matches(&success_promise()));
        assert!(!filter.matches(&error_promise()));
    }
}
