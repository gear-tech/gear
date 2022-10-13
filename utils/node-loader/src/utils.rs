use crate::{batch_pool::generators, SmallRng};
use anyhow::{anyhow, Result};
use dyn_clonable::*;
use futures::Future;
use futures_timer::Delay;
use gclient::{GearApi, WSAddress};
use rand::{Rng, RngCore, SeedableRng};
use std::{
    fs::File,
    io::Write,
    iter,
    ops::Deref,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub struct GearApiProducer {
    api: GearApi,
    nonce: u32,
}

impl GearApiProducer {
    pub async fn try_new(endpoint: String, user: String) -> Result<Self> {
        let api = GearApi::init_with(str_to_wsaddr(endpoint), user).await?;
        let nonce = api.rpc_nonce().await?;
        Ok(Self { api, nonce })
    }

    pub fn produce(&mut self) -> GearApi {
        let mut api = self.api.clone();
        api.set_nonce(self.nonce);
        self.nonce += 1;

        api
    }
}

pub fn now() -> u64 {
    let time_since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Internal error: current time before UNIX Epoch");

    time_since_epoch.as_millis() as u64
}

pub fn dump_with_seed(seed: u64) -> Result<()> {
    let code = generators::generate_gear_program::<SmallRng>(seed);

    let mut file = File::create("out.wasm")?;
    file.write_all(&code)?;

    Ok(())
}

pub fn str_to_wsaddr(endpoint: String) -> WSAddress {
    let endpoint = endpoint.replace("://", ":");

    let mut addr_parts = endpoint.split(':');

    let domain = format!(
        "{}://{}",
        addr_parts.next().unwrap_or("ws"),
        addr_parts.next().unwrap_or("127.0.0.1")
    );
    let port = addr_parts.next().and_then(|v| v.parse().ok());

    WSAddress::new(domain, port)
}

pub fn iterator_with_args<T, F: FnMut() -> T>(
    max_size: usize,
    mut args: F,
) -> impl Iterator<Item = T> {
    let mut size = 0;
    iter::from_fn(move || {
        if size >= max_size {
            return None;
        }

        size += 1;

        Some(args())
    })
}

#[clonable]
pub trait LoaderRngCore: RngCore + Clone {}
impl<T: RngCore + Clone> LoaderRngCore for T {}

pub trait LoaderRng: Rng + SeedableRng + 'static + Clone {}
impl<T: Rng + SeedableRng + 'static + Clone> LoaderRng for T {}

#[derive(Debug, Clone)]
pub struct NonEmptyVec<T>(Vec<T>);

impl<T> NonEmptyVec<T> {
    pub fn try_from_iter<I>(other: I) -> Result<Self, ()>
    where
        I: Iterator<Item = T>,
    {
        let mut peekable = other.peekable();
        (peekable.peek().is_some())
            .then_some(Self(peekable.collect()))
            .ok_or(())
    }

    pub fn ring_get(&self, index: usize) -> &T {
        assert!(!self.is_empty(), "NonEmptyVec instance is empty");
        &self[index % self.len()]
    }
}

impl<T> Deref for NonEmptyVec<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub async fn run_with_timeout<T>(fut: impl Future<Output = T>) -> Result<T> {
    // 1 minute as default
    let wait_task = Delay::new(Duration::from_millis(60_000));

    tokio::select! {
        output = fut => Ok(output),
        _ = wait_task => Err(anyhow!("Timeout"))
    }
}
