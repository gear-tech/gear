use anyhow::{Result, anyhow};
use ethexe_common::{gear::CodeState, injected::Promise};
use ethexe_ethereum::Ethereum;
use ethexe_sdk::VaraEthApi;
use gprimitives::{ActorId, CodeId, MessageId};
use rand::RngCore;
use tokio::time::{Duration, Instant};

pub(crate) const RPC_MAX_ATTEMPTS: usize = 3;
const RPC_RECONNECT_DELAY_MIN_SECS: u64 = 60;
const RPC_RECONNECT_DELAY_SPREAD_SECS: u64 = 60;
const CODE_VALIDATION_POLL_INTERVAL: Duration = Duration::from_secs(1);

struct EthexeRpcEndpoint {
    url: String,
    client: Option<VaraEthApi>,
    reconnect_not_before: Option<Instant>,
}

/// Small failover pool for ethexe JSON-RPC clients used by one worker.
pub(crate) struct EthexeRpcPool {
    endpoints: Vec<EthexeRpcEndpoint>,
}

fn all_codes_validated(states: &[CodeState]) -> bool {
    states.iter().all(|state| *state == CodeState::Validated)
}

impl EthexeRpcPool {
    /// Returns how many endpoints are configured in the pool.
    pub(crate) fn endpoint_count(&self) -> usize {
        self.endpoints.len()
    }

    /// Creates a new pool from the configured ethexe RPC URLs.
    pub(crate) fn new(urls: Vec<String>) -> Result<Self> {
        if urls.is_empty() {
            return Err(anyhow!(
                "at least one --ethexe-node endpoint must be provided"
            ));
        }

        let endpoints = urls
            .into_iter()
            .enumerate()
            .map(|(idx, url)| {
                tracing::info!(endpoint_idx = idx, endpoint = %url, "Configured ethexe RPC endpoint");
                EthexeRpcEndpoint {
                    url,
                    client: None,
                    reconnect_not_before: None,
                }
            })
            .collect();

        Ok(Self { endpoints })
    }

    /// Picks a preferred starting endpoint for a request.
    pub(crate) fn random_endpoint_index(&self, rng: &mut impl RngCore) -> usize {
        (rng.next_u32() as usize) % self.endpoints.len()
    }

    /// Establishes a fresh client connection for one endpoint.
    async fn reconnect_client(
        &mut self,
        endpoint_idx: usize,
        api: &Ethereum,
    ) -> Result<&VaraEthApi> {
        let endpoint = self
            .endpoints
            .get(endpoint_idx)
            .ok_or_else(|| anyhow!("invalid endpoint index: {endpoint_idx}"))?;

        tracing::warn!(
            endpoint_idx,
            endpoint = %endpoint.url,
            "Connecting ethexe RPC client"
        );

        let url = endpoint.url.clone();
        let client = VaraEthApi::new(&url, api.clone()).await?;

        let endpoint = &mut self.endpoints[endpoint_idx];
        endpoint.client = Some(client);
        endpoint.reconnect_not_before = None;

        tracing::info!(
            endpoint_idx,
            endpoint = %endpoint.url,
            "Connected ethexe RPC client"
        );

        Ok(endpoint.client.as_ref().expect("just inserted"))
    }

    /// Returns a connected client for an endpoint, respecting reconnect cooldowns.
    async fn get_or_connect_client(
        &mut self,
        endpoint_idx: usize,
        api: &Ethereum,
    ) -> Result<&VaraEthApi> {
        if endpoint_idx >= self.endpoints.len() {
            return Err(anyhow!("invalid endpoint index: {endpoint_idx}"));
        }

        let needs_connect = {
            let endpoint = &self.endpoints[endpoint_idx];
            if endpoint.client.is_some() {
                false
            } else if let Some(not_before) = endpoint.reconnect_not_before {
                let now = Instant::now();
                if now < not_before {
                    return Err(anyhow!(
                        "endpoint {endpoint_idx} reconnect is cooling down for {:?}",
                        not_before.duration_since(now)
                    ));
                }
                true
            } else {
                true
            }
        };

        if needs_connect {
            self.reconnect_client(endpoint_idx, api).await?;
        }

        Ok(self.endpoints[endpoint_idx].client.as_ref().unwrap())
    }

    /// Computes a deterministic reconnect delay for an endpoint.
    fn reconnect_delay_for_endpoint(endpoint_idx: usize) -> Duration {
        let spread = RPC_RECONNECT_DELAY_SPREAD_SECS.saturating_add(1);
        let jitter = (endpoint_idx as u64) % spread;
        Duration::from_secs(RPC_RECONNECT_DELAY_MIN_SECS.saturating_add(jitter))
    }

    /// Drops the current client and postpones reconnect attempts for an endpoint.
    fn schedule_reconnect(&mut self, endpoint_idx: usize, reason: &str) {
        if let Some(endpoint) = self.endpoints.get_mut(endpoint_idx) {
            endpoint.client = None;

            let delay = Self::reconnect_delay_for_endpoint(endpoint_idx);
            let not_before = Instant::now() + delay;
            endpoint.reconnect_not_before = Some(not_before);

            tracing::warn!(
                endpoint_idx,
                endpoint = %endpoint.url,
                reconnect_after_secs = delay.as_secs(),
                reason,
                "Scheduled delayed reconnect for ethexe RPC endpoint"
            );
        }
    }

    /// Returns endpoint indices in rotation order starting from `preferred_idx`.
    fn endpoint_indices_from(&self, preferred_idx: usize) -> Vec<usize> {
        let len = self.endpoints.len();
        if len == 0 {
            return Vec::new();
        }

        (0..len)
            .map(|offset| (preferred_idx + offset) % len)
            .collect()
    }

    /// Requests code validation through the ethexe RPC pool with failover.
    pub(crate) async fn request_code_validation(
        &mut self,
        preferred_endpoint_idx: usize,
        api: &Ethereum,
        code: &[u8],
    ) -> Result<CodeId> {
        let mut last_err: Option<anyhow::Error> = None;

        for attempt in 1..=RPC_MAX_ATTEMPTS {
            for endpoint_idx in self.endpoint_indices_from(preferred_endpoint_idx) {
                let client = match self.get_or_connect_client(endpoint_idx, api).await {
                    Ok(client) => client,
                    Err(err) => {
                        tracing::warn!(
                            endpoint_idx,
                            attempt,
                            max_attempts = RPC_MAX_ATTEMPTS,
                            error = %err,
                            "failed to acquire ethexe RPC client; will try another endpoint"
                        );
                        last_err = Some(err);
                        continue;
                    }
                };

                match client.router().request_code_validation(code).await {
                    Ok((_, code_id)) => return Ok(code_id),
                    Err(err) => {
                        tracing::warn!(
                            endpoint_idx,
                            attempt,
                            max_attempts = RPC_MAX_ATTEMPTS,
                            error = %err,
                            "request_code_validation failed; scheduling delayed reconnect"
                        );
                        self.schedule_reconnect(endpoint_idx, "request_code_validation failure");
                        last_err = Some(err);
                    }
                }
            }

            if attempt < RPC_MAX_ATTEMPTS {
                tracing::warn!(
                    attempt,
                    max_attempts = RPC_MAX_ATTEMPTS,
                    "request_code_validation retrying with available endpoints"
                );
            }
        }

        if let Some(err) = last_err {
            return Err(err);
        }

        Err(anyhow!("request_code_validation exhausted retries"))
    }

    /// Waits for previously requested code validations to finish with failover.
    pub(crate) async fn wait_for_codes_validation(
        &mut self,
        preferred_endpoint_idx: usize,
        api: &Ethereum,
        code_ids: impl IntoIterator<Item = CodeId>,
    ) -> Result<()> {
        let code_ids = code_ids.into_iter().collect::<Vec<_>>();
        if code_ids.is_empty() {
            return Ok(());
        }

        'poll: loop {
            let mut last_err: Option<anyhow::Error> = None;

            for attempt in 1..=RPC_MAX_ATTEMPTS {
                for endpoint_idx in self.endpoint_indices_from(preferred_endpoint_idx) {
                    let client = match self.get_or_connect_client(endpoint_idx, api).await {
                        Ok(client) => client,
                        Err(err) => {
                            tracing::warn!(
                                endpoint_idx,
                                attempt,
                                max_attempts = RPC_MAX_ATTEMPTS,
                                error = %err,
                                "failed to acquire ethexe RPC client; will try another endpoint"
                            );
                            last_err = Some(err);
                            continue;
                        }
                    };

                    match client.router().codes_states(code_ids.iter().copied()).await {
                        Ok(states) if all_codes_validated(&states) => return Ok(()),
                        Ok(states) => {
                            let pending = states
                                .iter()
                                .filter(|state| **state != CodeState::Validated)
                                .count();
                            tracing::debug!(
                                codes = code_ids.len(),
                                pending,
                                "Code validation is still pending"
                            );
                            tokio::time::sleep(CODE_VALIDATION_POLL_INTERVAL).await;
                            continue 'poll;
                        }
                        Err(err) => {
                            tracing::warn!(
                                endpoint_idx,
                                attempt,
                                max_attempts = RPC_MAX_ATTEMPTS,
                                error = %err,
                                "codes_states failed; scheduling delayed reconnect"
                            );
                            self.schedule_reconnect(endpoint_idx, "codes_states failure");
                            last_err = Some(err);
                        }
                    }
                }

                if attempt < RPC_MAX_ATTEMPTS {
                    tracing::warn!(
                        attempt,
                        max_attempts = RPC_MAX_ATTEMPTS,
                        "codes_states retrying with available endpoints"
                    );
                }
            }

            if let Some(err) = last_err {
                return Err(err);
            }

            return Err(anyhow!("codes_states exhausted retries"));
        }
    }

    /// Sends an injected message and waits for the promise returned by ethexe.
    pub(crate) async fn send_message_injected_and_watch(
        &mut self,
        preferred_endpoint_idx: usize,
        api: &Ethereum,
        actor: ActorId,
        payload: &[u8],
        value: u128,
    ) -> Result<(MessageId, Promise)> {
        let mut last_err: Option<anyhow::Error> = None;

        for attempt in 1..=RPC_MAX_ATTEMPTS {
            for endpoint_idx in self.endpoint_indices_from(preferred_endpoint_idx) {
                let client = match self.get_or_connect_client(endpoint_idx, api).await {
                    Ok(client) => client,
                    Err(err) => {
                        tracing::warn!(
                            endpoint_idx,
                            attempt,
                            max_attempts = RPC_MAX_ATTEMPTS,
                            error = %err,
                            "failed to acquire ethexe RPC client; will try another endpoint"
                        );
                        last_err = Some(err);
                        continue;
                    }
                };

                match client
                    .mirror(actor)
                    .send_message_injected_and_watch(payload, value)
                    .await
                {
                    Ok(result) => return Ok(result),
                    Err(err) => {
                        tracing::warn!(
                            endpoint_idx,
                            attempt,
                            max_attempts = RPC_MAX_ATTEMPTS,
                            error = %err,
                            "send_message_injected_and_watch failed; scheduling delayed reconnect"
                        );
                        self.schedule_reconnect(
                            endpoint_idx,
                            "send_message_injected_and_watch failure",
                        );
                        last_err = Some(err);
                    }
                }
            }

            if attempt < RPC_MAX_ATTEMPTS {
                tracing::warn!(
                    attempt,
                    max_attempts = RPC_MAX_ATTEMPTS,
                    "send_message_injected_and_watch retrying with available endpoints"
                );
            }
        }

        if let Some(err) = last_err {
            return Err(err);
        }

        Err(anyhow!("send_message_injected_and_watch exhausted retries"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::gear::CodeState;

    #[test]
    fn all_codes_validated_requires_every_state_to_be_validated() {
        assert!(all_codes_validated(&[
            CodeState::Validated,
            CodeState::Validated
        ]));
        assert!(!all_codes_validated(&[
            CodeState::Validated,
            CodeState::ValidationRequested
        ]));
        assert!(!all_codes_validated(&[
            CodeState::Validated,
            CodeState::Unknown
        ]));
    }
}
