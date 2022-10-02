use crate::{batch_pool::generators, SmallRng};
use dyn_clonable::*;
use gclient::{Result, WSAddress};
use rand::{Rng, RngCore, SeedableRng};
use std::{
    fs::File,
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
    ops::Deref,
};

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

#[clonable]
pub trait LoaderRngCore: RngCore + Clone {}
impl<T: RngCore + Clone> LoaderRngCore for T {}

pub trait LoaderRng: Rng + SeedableRng + 'static + Clone {}
impl<T: Rng + SeedableRng + 'static + Clone> LoaderRng for T {}

// Todo copy paste from property tests will be removed
pub trait RingGet<T> {
    fn ring_get(&self, index: usize) -> Option<&T>;
}

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
}

impl<T> Deref for NonEmptyVec<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> RingGet<T> for NonEmptyVec<T> {
    fn ring_get(&self, index: usize) -> Option<&T> {
        assert!(!self.is_empty(), "NonEmptyVec instance is empty");
        Some(&self[index % self.len()])
    }
}