// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Node-wide per-method RPC rate and concurrency budgets.

use std::{
    collections::{HashMap, HashSet},
    num::{NonZeroU32, NonZeroUsize},
    str::FromStr,
    sync::Arc,
};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::RateLimit;

/// A node-wide rate and concurrency limit for one or more RPC methods.
///
/// The first method is the canonical method used for metrics and logging. Every subsequent
/// method is an alias sharing the same budget.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RpcMethodLimit {
    methods: Vec<String>,
    calls_per_minute: NonZeroU32,
    max_in_flight: NonZeroUsize,
}

impl RpcMethodLimit {
    /// Validates that every method belongs to at most one budget group.
    pub fn validate_all(limits: &[Self]) -> Result<(), String> {
        let mut seen = HashSet::new();
        for limit in limits {
            for method in &limit.methods {
                if !seen.insert(method) {
                    return Err(format!(
                        "RPC method `{method}` appears in more than one method-limit group"
                    ));
                }
            }
        }
        Ok(())
    }
}

impl FromStr for RpcMethodLimit {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (methods, limits) = value.split_once('=').ok_or_else(|| {
            "RPC method limit must use METHOD[,ALIAS...]=CALLS_PER_MINUTE,MAX_IN_FLIGHT".to_owned()
        })?;

        let methods = methods.split(',').map(str::to_owned).collect::<Vec<_>>();
        if methods.is_empty() || methods.iter().any(String::is_empty) {
            return Err("RPC method limit contains an empty method name or alias".to_owned());
        }

        let values = limits.split(',').collect::<Vec<_>>();
        if values.len() != 2 || values.iter().any(|value| value.is_empty()) {
            return Err(
                "RPC method limit must contain exactly two non-zero numbers after `=`".to_owned(),
            );
        }

        let calls_per_minute = values[0].parse::<NonZeroU32>().map_err(|_| {
            "RPC method limit calls-per-minute must be a non-zero 32-bit integer".to_owned()
        })?;
        let max_in_flight = values[1].parse::<NonZeroUsize>().map_err(|_| {
            "RPC method limit max-in-flight must be a non-zero usize integer".to_owned()
        })?;
        if max_in_flight.get() > Semaphore::MAX_PERMITS {
            return Err(format!(
                "RPC method limit max-in-flight must not exceed {}",
                Semaphore::MAX_PERMITS
            ));
        }

        Ok(Self {
            methods,
            calls_per_minute,
            max_in_flight,
        })
    }
}

#[derive(Debug)]
struct MethodLimiter {
    canonical: Arc<str>,
    max_in_flight: usize,
    semaphore: Arc<Semaphore>,
    rate_limit: RateLimit,
}

/// Shared node-wide method limiter registry.
#[derive(Debug)]
pub(crate) struct MethodLimiters {
    methods: HashMap<String, Arc<MethodLimiter>>,
}

impl MethodLimiters {
    /// Builds the registry and validates duplicate method names.
    pub(crate) fn try_new(limits: Vec<RpcMethodLimit>) -> Result<Self, String> {
        RpcMethodLimit::validate_all(&limits)?;

        let mut methods = HashMap::new();
        for limit in limits {
            log::info!(
                target: "rpc",
                "Configured RPC method limit: methods={:?}, calls_per_minute={}, max_in_flight={}",
                limit.methods,
                limit.calls_per_minute,
                limit.max_in_flight,
            );

            let canonical: Arc<str> = Arc::from(limit.methods[0].as_str());
            let max_in_flight = limit.max_in_flight.get();
            let limiter = Arc::new(MethodLimiter {
                canonical: canonical.clone(),
                max_in_flight,
                semaphore: Arc::new(Semaphore::new(max_in_flight)),
                rate_limit: RateLimit::per_minute(limit.calls_per_minute),
            });

            for method in limit.methods {
                methods.insert(method, limiter.clone());
            }
        }

        Ok(Self { methods })
    }

    /// Returns the canonical method for a configured literal method name.
    pub(crate) fn canonical(&self, method: &str) -> Option<Arc<str>> {
        self.methods
            .get(method)
            .map(|limiter| limiter.canonical.clone())
    }

    /// Returns the current in-flight count and maximum for a configured method.
    ///
    /// Literal aliases resolve to the same limiter and therefore share this
    /// snapshot. Unconfigured methods return `None`.
    pub(crate) fn concurrency(&self, method: &str) -> Option<(usize, usize)> {
        let limiter = self.methods.get(method)?;
        let max_in_flight = limiter.max_in_flight;
        let in_flight = max_in_flight.saturating_sub(limiter.semaphore.available_permits());
        Some((in_flight, max_in_flight))
    }

    /// Tries to admit one call without waiting.
    ///
    /// An unconfigured method returns `Ok(None)`. A configured method returns an
    /// owned semaphore permit on success; the permit is held by the admission
    /// until the request future completes or is dropped.
    pub(crate) fn try_acquire(
        &self,
        method: &str,
    ) -> Result<Option<MethodAdmission>, MethodLimitReason> {
        let Some(limiter) = self.methods.get(method) else {
            return Ok(None);
        };

        let permit = limiter
            .semaphore
            .clone()
            .try_acquire_owned()
            .map_err(|_| MethodLimitReason::Concurrency)?;

        if limiter.rate_limit.inner.check().is_err() {
            // Dropping the owned permit immediately releases the concurrency slot
            // before returning the rejection.
            drop(permit);
            return Err(MethodLimitReason::Rate);
        }

        Ok(Some(MethodAdmission {
            canonical: limiter.canonical.clone(),
            _permit: permit,
        }))
    }
}

/// The reason a configured method call was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MethodLimitReason {
    /// The method's calls-per-minute bucket is exhausted.
    Rate,
    /// The method's in-flight semaphore is exhausted.
    Concurrency,
}

impl MethodLimitReason {
    /// Returns the bounded metric/log label for this reason.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Rate => "rate",
            Self::Concurrency => "concurrency",
        }
    }
}

/// An admitted method call and its owned in-flight permit.
#[derive(Debug)]
pub(crate) struct MethodAdmission {
    pub(crate) canonical: Arc<str>,
    pub(crate) _permit: OwnedSemaphorePermit,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(value: &str) -> RpcMethodLimit {
        value.parse().expect("valid RPC method limit")
    }

    #[test]
    fn parses_single_method_and_aliases() {
        let limit = parse("state_getRuntimeVersion,chain_getRuntimeVersion=60,2");
        assert_eq!(limit.methods.len(), 2);
        assert_eq!(limit.methods[0], "state_getRuntimeVersion");
        assert_eq!(limit.methods[1], "chain_getRuntimeVersion");
        assert_eq!(limit.calls_per_minute.get(), 60);
        assert_eq!(limit.max_in_flight.get(), 2);
    }

    #[test]
    fn validates_repeated_groups_and_rejects_invalid_values() {
        let repeated = vec![
            parse("state_getMetadata=30,1"),
            parse("state_getMetadata=10,2"),
        ];
        assert!(RpcMethodLimit::validate_all(&repeated).is_err());

        for value in [
            "state_getMetadata=0,1",
            "state_getMetadata=1,0",
            "state_getMetadata=1",
            "state_getMetadata=1,2,3",
            "state_getMetadata,,state_getRuntimeVersion=1,2",
            "=1,2",
            "state_getMetadata=,2",
            "state_getMetadata=1,",
            "state_getMetadata",
        ] {
            assert!(
                value.parse::<RpcMethodLimit>().is_err(),
                "{value} should fail"
            );
        }

        let too_many_permits = format!(
            "state_getMetadata=1,{}",
            Semaphore::MAX_PERMITS.saturating_add(1)
        );
        assert!(too_many_permits.parse::<RpcMethodLimit>().is_err());
    }

    #[test]
    fn aliases_share_permit_and_methods_are_independent() {
        let limits = vec![
            parse("state_getRuntimeVersion,chain_getRuntimeVersion=60,1"),
            parse("state_getMetadata=60,1"),
        ];
        let limiters = MethodLimiters::try_new(limits).unwrap();

        let first = limiters
            .try_acquire("state_getRuntimeVersion")
            .unwrap()
            .unwrap();
        assert_eq!(&*first.canonical, "state_getRuntimeVersion");
        assert!(matches!(
            limiters.try_acquire("chain_getRuntimeVersion"),
            Err(MethodLimitReason::Concurrency)
        ));
        assert!(limiters.try_acquire("state_getMetadata").unwrap().is_some());
        drop(first);
        assert!(limiters
            .try_acquire("chain_getRuntimeVersion")
            .unwrap()
            .is_some());
        assert!(limiters.try_acquire("system_health").unwrap().is_none());
    }

    #[test]
    fn concurrency_snapshot_tracks_aliases_and_permit_lifetime() {
        let limiters = MethodLimiters::try_new(vec![parse(
            "state_getRuntimeVersion,chain_getRuntimeVersion=60,2",
        )])
        .unwrap();

        assert_eq!(limiters.concurrency("system_health"), None);
        assert_eq!(
            limiters.concurrency("state_getRuntimeVersion"),
            Some((0, 2))
        );

        let admission = limiters
            .try_acquire("state_getRuntimeVersion")
            .unwrap()
            .unwrap();
        assert_eq!(
            limiters.concurrency("chain_getRuntimeVersion"),
            Some((1, 2))
        );

        drop(admission);
        assert_eq!(
            limiters.concurrency("state_getRuntimeVersion"),
            Some((0, 2))
        );
    }

    #[test]
    fn rate_rejection_is_immediate_and_releases_permit() {
        let limiters = MethodLimiters::try_new(vec![parse("state_getMetadata=1,1")]).unwrap();
        let first = limiters.try_acquire("state_getMetadata").unwrap().unwrap();
        assert!(matches!(
            limiters.try_acquire("state_getMetadata"),
            Err(MethodLimitReason::Concurrency)
        ));
        drop(first);
        assert!(matches!(
            limiters.try_acquire("state_getMetadata"),
            Err(MethodLimitReason::Rate)
        ));
        assert!(limiters.try_acquire("state_getMetadata").is_err());
    }
}
