use anyhow::{Result, anyhow};
use gclient::Result as GClientResult;
use once_cell::sync::OnceCell;
use parking_lot::{Mutex, MutexGuard};
use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    sync::atomic::{AtomicU32, Ordering},
};
use crate::utils;

pub static AVAILABLE_NONCE: OnceCell<AtomicU32> = OnceCell::new();
pub static MISSED_NONCES: OnceCell<Mutex<MinHeap>> = OnceCell::new();

pub type MinHeap = BinaryHeap<Reverse<u32>>;
type MissedNoncesGuard<'a> = MutexGuard<'a, MinHeap>;


pub fn is_empty_missed_nonce() -> Result<bool> {
    hold_missed_nonces().map(|mn| mn.is_empty())
}

pub fn increment_nonce() -> Result<u32> {
    AVAILABLE_NONCE
        .get()
        .ok_or_else(|| anyhow!("Not initialized missed nonces storage"))
        .map(|an| an.fetch_add(1, Ordering::Relaxed))
}

pub fn pop_missed_nonce() -> Result<u32> {
    hold_missed_nonces()?
        .pop()
        .map(|Reverse(v)| v)
        .ok_or_else(|| anyhow!("empty missed nonce storage"))
}

fn hold_missed_nonces<'a>() -> Result<MissedNoncesGuard<'a>> {
    MISSED_NONCES
        .get()
        .map(|m| m.lock())
        .ok_or_else(|| anyhow!("Not initialized missed nonces storage"))
}

pub fn catch_missed_nonce<T>(batch_res: &GClientResult<T>, nonce: u32) -> Result<()> {
    if let Err(err) = batch_res {
        if err.to_string().contains(utils::SUBXT_RPC_CALL_ERR_STR) {
            hold_missed_nonces()?.push(Reverse(nonce));
        }
    }

    Ok(())
}

#[test]
fn test_min_heap_order() {
    use rand::Rng;

    let mut test_array = [0u32; 512];
    let mut thread_rng = rand::thread_rng();
    thread_rng.fill(&mut test_array);

    let mut min_heap = MinHeap::from_iter(test_array.into_iter().map(Reverse));

    test_array.sort_unstable();

    for expected in test_array {
        let actual = min_heap.pop().expect("same size as iterator");
        assert_eq!(
            Reverse(expected),
            actual,
            "failed test with test array {test_array:?}"
        );
    }
}