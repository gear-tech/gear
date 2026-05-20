// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{RpcEvent, errors};
use ethexe_common::{
    Address,
    injected::{AddressedInjectedTransaction, InjectedTransactionAcceptance},
};
use ethexe_db::Database;
use jsonrpsee::core::RpcResult;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, trace, warn};

#[derive(Clone)]
pub struct TransactionsRelayer {
    rpc_sender: mpsc::UnboundedSender<RpcEvent>,
    db: Database,
}

impl TransactionsRelayer {
    pub fn new(rpc_sender: mpsc::UnboundedSender<RpcEvent>, db: Database) -> Self {
        Self { rpc_sender, db }
    }

    pub async fn relay(
        &self,
        mut transaction: AddressedInjectedTransaction,
    ) -> RpcResult<InjectedTransactionAcceptance> {
        let tx_hash = transaction.tx.data().to_hash();
        trace!(%tx_hash, ?transaction, "Called injected_sendTransaction with vars");

        if transaction.tx.data().value != 0 {
            warn!(
                tx_hash = %tx_hash,
                value = transaction.tx.data().value,
                "Injected transaction with non-zero value is not supported"
            );
            return Err(errors::bad_request(
                "Injected transactions with non-zero value are not supported",
            ));
        }

        if transaction.recipient == Address::default() {
            utils::route_transaction(&self.db, &mut transaction)?;
        }

        let (response_sender, response_receiver) = oneshot::channel();
        let event = RpcEvent::InjectedTransaction {
            transaction,
            response_sender,
        };

        if let Err(err) = self.rpc_sender.send(event) {
            error!(
                "Failed to send `RpcEvent::InjectedTransaction` event task: {err}. \
                The receiving end in the main service might have been dropped."
            );
            return Err(errors::internal());
        }

        trace!(%tx_hash, "Accept transaction, waiting for promise");

        response_receiver.await.map_err(|err| {
            // Expecting no errors here, because the rpc channel is owned by main server.
            error!("Response sender for the `RpcEvent::InjectedTransaction` was dropped: {err}");
            errors::internal()
        })
    }
}

mod utils {
    use super::*;
    use anyhow::{Context as _, Result};
    use ethexe_common::{
        Address,
        db::{ConfigStorageRO, OnChainStorageRO},
    };
    use std::time::{Duration, SystemTime, SystemTimeError};

    pub(super) const NEXT_PRODUCER_THRESHOLD_MS: u64 = 50;

    pub fn route_transaction(
        db: &Database,
        tx: &mut AddressedInjectedTransaction,
    ) -> RpcResult<()> {
        let now = now_since_unix_epoch().map_err(|err| {
            error!("system clock error: {err}");
            crate::errors::internal()
        })?;

        let next_producer = calculate_next_producer(db, now).map_err(|err| {
            error!("calculate next producer error: {err}");
            crate::errors::internal()
        })?;
        tx.recipient = next_producer;

        Ok(())
    }

    /// Calculates the producer address to route an injected transaction to.
    pub(super) fn calculate_next_producer(db: &Database, now: Duration) -> Result<Address> {
        let timelines = db.config().timelines;

        // Calculate target timestamp, taking into account possible delays, so we append NEXT_PRODUCER_THRESHOLD_MS.
        // The transaction should be included by the next producer, so we add `slot_duration` to the current time.
        let target_timestamp = now
            .checked_add(Duration::from_millis(NEXT_PRODUCER_THRESHOLD_MS))
            .context("current time is too close to u64::MAX, cannot calculate next producer")?
            .as_secs()
            .checked_add(timelines.slot.get())
            .context("current time is too close to u64::MAX, cannot calculate next producer")?;

        let era = timelines
            .era_from_ts(target_timestamp)
            .context("failed to calculate era from target timestamp")?;

        let validators = db
            .validators(era)
            .with_context(|| format!("validators not found for era={era}"))?;

        timelines
            .block_producer_at(&validators, target_timestamp)
            .context("failed to calculate block producer")
    }

    /// Returns the current time since [SystemTime::UNIX_EPOCH].
    fn now_since_unix_epoch() -> Result<Duration, SystemTimeError> {
        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
    }
}

#[cfg(test)]
mod tests {
    use super::utils;
    use ethexe_common::{
        Address, ProtocolTimelines, ValidatorsVec,
        db::{ConfigStorageRO, OnChainStorageRW, SetConfig},
    };
    use ethexe_db::Database;
    use gear_core::pages::num_traits::ToPrimitive;
    use std::{ops::Sub, time::Duration};

    const SLOT: u64 = 10;
    const ERA: u64 = 1000;

    fn setup_db(db: &Database) -> ValidatorsVec {
        let validators = ValidatorsVec::from_iter((0..10u64).map(Address::from));

        let timelines = ProtocolTimelines {
            genesis_ts: 0,
            era: ERA.try_into().unwrap(),
            election: 0,
            slot: SLOT.try_into().unwrap(),
        };
        db.set_validators(0, validators.clone());
        let mut config = db.config().clone();
        config.timelines = timelines;
        db.set_config(config);
        validators
    }

    #[test]
    fn test_calculate_next_producer_return_next() {
        let db = Database::memory();
        let validators = setup_db(&db);

        let now = Duration::from_secs(SLOT / 2);
        let producer = utils::calculate_next_producer(&db, now).unwrap();

        assert_eq!(validators[1], producer);
    }

    #[test]
    fn test_calculate_next_producer_return_next_next() {
        let db = Database::memory();
        let validators = setup_db(&db);

        let half_threshold = utils::NEXT_PRODUCER_THRESHOLD_MS.to_u64().unwrap();
        let now = Duration::from_secs(SLOT).sub(Duration::from_millis(half_threshold));
        let producer = utils::calculate_next_producer(&db, now).unwrap();

        assert_eq!(validators[2], producer);
    }

    #[test]
    fn test_calculate_next_producer_in_next_era() {
        let db = Database::memory();
        let validators = setup_db(&db);

        // Prepare next era validators
        let mut next_era_validators = validators.clone();
        next_era_validators[0] = validators[9];
        db.set_validators(1, next_era_validators.clone());

        let now = Duration::from_secs(ERA).sub(Duration::from_secs(1));
        let producer = utils::calculate_next_producer(&db, now).unwrap();

        assert_eq!(next_era_validators[0], producer);
    }
}
