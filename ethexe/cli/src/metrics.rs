// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::time::SystemTime;

use crate::config::Config;
use ethexe_observer::ObserverStatus;
use ethexe_prometheus_endpoint::{Gauge, Opts, PrometheusError, Registry, U64, register};
use ethexe_sequencer::SequencerStatus;
use ethexe_utils::metrics::register_globals;
use futures_timer::Delay;
use std::time::{Duration, Instant};
use tokio::sync::watch;

struct PrometheusMetrics {
    // generic info
    eth_block_height: Gauge<U64>,
    pending_upload_code: Gauge<U64>,
    last_router_state: Gauge<U64>,
    aggregated_commitments: Gauge<U64>,
    submitted_code_commitments: Gauge<U64>,
    submitted_block_commitments: Gauge<U64>,
}

impl PrometheusMetrics {
    fn setup(registry: &Registry, name: &str) -> Result<Self, PrometheusError> {
        register(
            Gauge::<U64>::with_opts(
                Opts::new(
                    "ethexe_build_info",
                    "A metric with a constant '1' value labeled by name, version",
                )
                .const_label("name", name),
            )?,
            registry,
        )?
        .set(1);

        register_globals(registry)?;

        let start_time_since_epoch = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        register(
            Gauge::<U64>::new(
                "ethexe_process_start_time_seconds",
                "Number of seconds between the UNIX epoch and the moment the process started",
            )?,
            registry,
        )?
        .set(start_time_since_epoch.as_secs());

        Ok(Self {
            // generic internals
            eth_block_height: register(
                Gauge::<U64>::new(
                    "ethexe_eth_block_height",
                    "Block height info of the ethereum observer",
                )?,
                registry,
            )?,

            pending_upload_code: register(
                Gauge::<U64>::new(
                    "ethexe_pending_upload_code",
                    "Pending upload code events of the ethereum observer",
                )?,
                registry,
            )?,

            last_router_state: register(
                Gauge::<U64>::new(
                    "ethexe_last_router_state",
                    "Block height of the latest state of the router contract",
                )?,
                registry,
            )?,

            aggregated_commitments: register(
                Gauge::<U64>::new(
                    "ethexe_aggregated_commitments",
                    "Number of commitments aggregated in sequencer",
                )?,
                registry,
            )?,

            submitted_code_commitments: register(
                Gauge::<U64>::new(
                    "ethexe_submitted_code_commitments",
                    "Number of submitted code commitments in sequencer",
                )?,
                registry,
            )?,

            submitted_block_commitments: register(
                Gauge::<U64>::new(
                    "ethexe_submitted_block_commitments",
                    "Number of submitted block commitments in sequencer",
                )?,
                registry,
            )?,
        })
    }
}

/// A `MetricsService` periodically sends general client and
/// network state to the telemetry as well as (optionally)
/// a Prometheus endpoint.
pub struct MetricsService {
    metrics: Option<PrometheusMetrics>,
    last_update: Instant,
}

impl MetricsService {
    /// Creates a `MetricsService` that sends metrics
    /// to prometheus alongside the telemetry.
    pub fn with_prometheus(registry: &Registry, config: &Config) -> Result<Self, PrometheusError> {
        PrometheusMetrics::setup(registry, &config.node_name).map(|p| MetricsService {
            metrics: Some(p),
            last_update: Instant::now(),
        })
    }

    /// Returns a never-ending `Future` that performs the
    /// metric and telemetry updates with information from
    /// the given sources.
    pub async fn run(
        mut self,
        mut observer_status: watch::Receiver<ObserverStatus>,
        mut sequencer_status: Option<watch::Receiver<SequencerStatus>>,
    ) {
        let mut timer = Delay::new(Duration::from_secs(0));
        let timer_interval = Duration::from_secs(5);

        loop {
            // Wait for the next tick of the timer.
            (&mut timer).await;

            // Update / Send the metrics.
            self.update(
                *observer_status.borrow_and_update(),
                sequencer_status.as_mut().map(|s| *s.borrow_and_update()),
            );

            // Schedule next tick.
            timer.reset(timer_interval);
        }
    }

    fn update(
        &mut self,
        observer_status: ObserverStatus,
        sequencer_status: Option<SequencerStatus>,
    ) {
        let now = Instant::now();
        self.last_update = now;

        let eth_number: u64 = observer_status.eth_block_number;
        let pending_upload_code: u64 = observer_status.pending_upload_code;
        let last_router_state: u64 = observer_status.last_router_state;

        if let Some(metrics) = self.metrics.as_ref() {
            metrics.eth_block_height.set(eth_number);
            metrics.pending_upload_code.set(pending_upload_code);
            metrics.last_router_state.set(last_router_state);
            log::trace!("Observer status: {:?}", observer_status);
            if let Some(sequencer_status) = sequencer_status {
                metrics
                    .aggregated_commitments
                    .set(sequencer_status.aggregated_commitments);
                metrics
                    .submitted_code_commitments
                    .set(sequencer_status.submitted_code_commitments);
                metrics
                    .submitted_block_commitments
                    .set(sequencer_status.submitted_block_commitments);
                log::trace!("Sequencer status: {:?}", sequencer_status);
            }
        }

        // TODO: Use network status
        // Update/send network status information, if any.
    }
}
