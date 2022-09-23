use anyhow::Result;
use dyn_clonable::*;
use gear_program::{api::Api, keystore};
use rand::{Rng, RngCore, SeedableRng};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn now() -> u64 {
    let time_since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("internal error: current time before UNIX Epoch");
    time_since_epoch.as_millis() as u64
}

pub(crate) async fn obtain_gear_api(endpoint: &str, user: &str) -> Result<Api> {
    keystore::login(user, None)?;
    Api::new(Some(endpoint)).await.map_err(|e| e.into())
}

#[clonable]
pub(crate) trait LoaderRngCore: RngCore + Clone {}
impl<T: RngCore + Clone> LoaderRngCore for T {}

pub(crate) trait LoaderRng: Rng + SeedableRng + 'static + Clone {}
impl<T: Rng + SeedableRng + 'static + Clone> LoaderRng for T {}
