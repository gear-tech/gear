// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_common::injected::TransactionAcceptance;
use std::num::NonZeroUsize;
use tokio::sync::oneshot;

/// This struct holds pending transaction senders ([oneshot::Sender]).
///
/// Transaction senders waits for acceptance/reject from other validators in
/// network.
pub(super) struct PendingNetworkInjectedTx {
    response_senders: Vec<oneshot::Sender<TransactionAcceptance>>,
    pending_responses: usize,
    last_reject: Option<TransactionAcceptance>,
}

impl PendingNetworkInjectedTx {
    pub(super) fn new(
        response_sender: oneshot::Sender<TransactionAcceptance>,
        pending_responses: NonZeroUsize,
        last_reject: Option<TransactionAcceptance>,
    ) -> Self {
        Self {
            response_senders: vec![response_sender],
            pending_responses: pending_responses.get(),
            last_reject,
        }
    }

    pub(super) fn add_response_sender(
        &mut self,
        response_sender: oneshot::Sender<TransactionAcceptance>,
    ) {
        self.response_senders.push(response_sender);
    }

    pub(super) fn into_response_senders(self) -> Vec<oneshot::Sender<TransactionAcceptance>> {
        self.response_senders
    }

    pub(super) fn record_response(
        &mut self,
        acceptance: TransactionAcceptance,
    ) -> Option<TransactionAcceptance> {
        match acceptance {
            TransactionAcceptance::Accept => Some(TransactionAcceptance::Accept),
            rejection @ TransactionAcceptance::Reject { .. } => {
                // Infallible because in case of `self.pending_responses == 0` returns `Some`.
                self.pending_responses = self.pending_responses.checked_sub(1).expect("infallible");
                self.last_reject = Some(rejection);

                if self.pending_responses == 0 {
                    // Infallible, because was received at least 1 rejection.
                    Some(self.last_reject.take().expect("infallible"))
                } else {
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response_sender() -> oneshot::Sender<TransactionAcceptance> {
        oneshot::channel().0
    }

    #[test]
    fn returns_accept_immediately() {
        let mut pending = PendingNetworkInjectedTx::new(
            response_sender(),
            NonZeroUsize::new(2).expect("non-zero"),
            None,
        );

        let acceptance = pending.record_response(TransactionAcceptance::Accept);

        assert_eq!(acceptance, Some(TransactionAcceptance::Accept));
    }

    #[test]
    fn returns_last_reject_after_all_responses() {
        let mut pending = PendingNetworkInjectedTx::new(
            response_sender(),
            NonZeroUsize::new(2).expect("non-zero"),
            Some(TransactionAcceptance::Reject {
                reason: "local".into(),
            }),
        );

        let acceptance = pending.record_response(TransactionAcceptance::Reject {
            reason: "remote-1".into(),
        });

        assert_eq!(acceptance, None);

        let acceptance = pending.record_response(TransactionAcceptance::Reject {
            reason: "remote-2".into(),
        });

        assert_eq!(
            acceptance,
            Some(TransactionAcceptance::Reject {
                reason: "remote-2".into()
            })
        );
    }

    #[test]
    fn returns_reject_when_initialized_without_last_reject() {
        let mut pending = PendingNetworkInjectedTx::new(
            response_sender(),
            NonZeroUsize::new(1).expect("non-zero"),
            None,
        );

        let acceptance = pending.record_response(TransactionAcceptance::Reject {
            reason: "remote".into(),
        });

        assert_eq!(
            acceptance,
            Some(TransactionAcceptance::Reject {
                reason: "remote".into()
            })
        );
    }
}
