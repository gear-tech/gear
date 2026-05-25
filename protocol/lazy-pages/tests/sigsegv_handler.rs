// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Regression test: the lazy-pages fault handler must stay async-signal-safe
//! for a fault raised outside any lazy-pages-managed memory.
//!
//! `gear_lazy_pages::init` installs a process-wide SIGSEGV handler. The
//! handler classifies the fault by address first: a fault outside the WASM
//! memory lazy-pages currently protects on the faulting thread is forwarded
//! to the previous handler / the kernel's default action, without touching
//! thread-locals or logging. Before that classification existed, such a
//! fault — e.g. a SIGSEGV from a RocksDB worker thread — reached the full
//! handler path and `panic!`ed (`RuntimeContextIsNotSet`); panicking inside
//! a fault handler is not async-signal-safe and aborted otherwise-healthy
//! validator nodes.
//!
//! This test raises a SIGSEGV on a thread running no lazy-pages execution
//! and asserts the process dies cleanly via the default action, not via a
//! handler panic.
//!
//! The handler intercepts SIGSEGV only on Linux (on other unixes it is
//! installed for SIGBUS), so the regression is reproducible on Linux only.

#![cfg(target_os = "linux")]

use gear_core::limited::LimitedStr;
use gear_lazy_pages::{LazyPagesStorage, LazyPagesVersion};
use gear_lazy_pages_common::LazyPagesInitContext;
use std::{env, os::unix::process::ExitStatusExt, process::Command, thread};

/// Set on the re-executed test binary so it plays the faulting child.
const CHILD_ENV: &str = "LAZY_PAGES_SIGSEGV_REGRESSION_CHILD";

/// Exact name of the test below — used to re-exec the test binary.
const TEST_NAME: &str = "handler_forwards_fault_outside_managed_region";

const WASM_PAGE_SIZE: u32 = 0x10000;
const GEAR_PAGE_SIZE: u32 = 0x4000;

#[derive(Debug)]
struct NoopStorage;

impl LazyPagesStorage for NoopStorage {
    fn page_exists(&self, _key: &[u8]) -> bool {
        unreachable!("a fault outside managed memory never reaches page storage")
    }

    fn load_page(&mut self, _key: &[u8], _buffer: &mut [u8]) -> Option<u32> {
        unreachable!("a fault outside managed memory never reaches page storage")
    }
}

/// Installs the lazy-pages handler like ethexe does, then raises a SIGSEGV on
/// a thread that holds no lazy-pages runtime context. The fault terminates the
/// whole process, so this never returns normally.
fn run_faulting_child() {
    let init_ctx = LazyPagesInitContext {
        page_sizes: vec![WASM_PAGE_SIZE, GEAR_PAGE_SIZE],
        global_names: vec![LimitedStr::from_small_str("gear_gas")],
        pages_storage_prefix: Default::default(),
    };
    gear_lazy_pages::init(LazyPagesVersion::Version1, init_ctx, NoopStorage)
        .expect("lazy-pages init must succeed");

    eprintln!("child: lazy-pages handler installed; faulting on a context-less thread");

    // This thread never calls `init`, so its thread-local lazy-pages state
    // stays empty (no runtime context, no managed region) — exactly like a
    // RocksDB or RPC worker thread.
    let faulting_thread = thread::spawn(|| {
        let wild_ptr = std::ptr::null::<u8>();
        // SAFETY: dereferencing an unmapped address on purpose, to raise a
        // genuine SIGSEGV outside any lazy-pages-managed region.
        let byte = unsafe { std::ptr::read_volatile(wild_ptr) };
        std::hint::black_box(byte);
    });
    let _ = faulting_thread.join();

    panic!("the wild memory access was expected to terminate the process");
}

#[test]
fn handler_forwards_fault_outside_managed_region() {
    if env::var(CHILD_ENV).is_ok() {
        run_faulting_child();
        return;
    }

    if !cfg!(target_os = "linux") {
        eprintln!("skipped: lazy-pages SIGSEGV regression is Linux-specific");
        return;
    }

    let test_binary = env::current_exe().expect("path to this test binary");
    let output = Command::new(test_binary)
        .args([TEST_NAME, "--exact", "--nocapture"])
        .env(CHILD_ENV, "1")
        .output()
        .expect("re-exec the test binary as the faulting child");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // A regression in fault classification would route this foreign fault
    // into the full handler path, which `panic!`s with `RuntimeContextIsNotSet`.
    assert!(
        !stderr.contains("RuntimeContextIsNotSet"),
        "lazy-pages handler panicked on a fault outside any managed region \
         instead of forwarding it\n--- child stderr ---\n{stderr}"
    );
    // Panicking inside the signal handler is not async-signal-safe and ends
    // in this fatal runtime error — it must not appear.
    assert!(
        !stderr.contains("failed to initiate panic"),
        "signal handler panicked instead of forwarding the fault\n\
         --- child stderr ---\n{stderr}"
    );
    // The forwarded fault must still crash the process via the default
    // SIGSEGV action, not let it exit cleanly or hang.
    assert!(
        output.status.signal().is_some(),
        "child was expected to be terminated by a signal, got {:?}\n\
         --- child stderr ---\n{stderr}",
        output.status,
    );
}
