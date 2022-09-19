use gear_program::{api::Api, keystore};
use rand::{RngCore, SeedableRng};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn now() -> u64 {
    let time_since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("internal error: current time before UNIX Epoch");
    time_since_epoch.as_millis() as u64
}

pub(crate) async fn obtain_gear_api(endpoint: &str, user: &str) -> Result<Api, String> {
    keystore::login(user, None).map_err(|e| e.to_string())?;
    Api::new(Some(endpoint)).await.map_err(|e| e.to_string())
}

pub(crate) trait Rng: RngCore + SeedableRng + 'static {}
impl<T: RngCore + SeedableRng + 'static> Rng for T {}

