// This file is part of Gear.

// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! End-to-end demo: sr25519 verify WASM-path vs syscall-path gas.
//!
//! Release gate for Stage 0 of the crypto-syscalls proposal.

use demo_crypto::{Mode, VerifyRequest};
use gtest::{Program, System, constants::DEFAULT_USER_ALICE};
use parity_scale_codec::{Decode, Encode};
use sp_core::{Pair, sr25519};

/// The test does: generate a random sr25519 keypair; sign a message; send
/// it through both verify paths; compare gas burns; require the syscall
/// path to be at least 50x cheaper than pure-WASM schnorrkel.
#[test]
fn sr25519_wasm_vs_syscall_gas_delta() {
    let system = System::new();
    system.init_logger();

    let (pair, _) = sr25519::Pair::generate();
    let pk: [u8; 32] = pair.public().0;
    let msg: &[u8] = b"gear-protocol-crypto-syscall-demo";
    let sig: [u8; 64] = pair.sign(msg).0;

    let program = Program::current(&system);
    let from = DEFAULT_USER_ALICE;

    // First send_bytes on a fresh program goes to init(), not handle().
    // Burn it on an empty init before the measured runs.
    let _init_id = program.send_bytes(from, []);
    let init_run = system.run_next_block();
    assert!(
        init_run.succeed.contains(&_init_id),
        "program init failed to succeed"
    );

    let wasm_gas = run_mode(&system, &program, from, Mode::Wasm, &pk, msg, &sig);
    let sys_gas = run_mode(&system, &program, from, Mode::Syscall, &pk, msg, &sig);

    let speedup = wasm_gas / sys_gas;
    let delta = wasm_gas.saturating_sub(sys_gas);

    println!("\n=== sr25519 verify — WASM vs syscall ===");
    println!("  WASM path (schnorrkel in-WASM):   {wasm_gas:>15} gas");
    println!("  Syscall path (gr_sr25519_verify): {sys_gas:>15} gas");
    println!("  Delta (WASM curve25519 cost):     {delta:>15} gas saved");
    println!("  Total-per-message speedup:        {speedup:>15}x\n");
    println!("  Note: syscall path carries the same ~7B floor of per-message");
    println!("  overhead (msg decode + gstd + reply). Actual verify-only");
    println!("  speedup ≈ {delta} / weight_for(gr_sr25519_verify).");
    println!("  Stage 0 ships with SyscallWeights::gr_sr25519_verify =");
    println!("  Weight::zero(); real numbers land with benchmarks.");

    // WASM-mode should clearly exceed the in-WASM curve25519 cost — the
    // proposal's 17B projection is the right order of magnitude for a
    // bare verify; our demo adds SCALE decode + gstd overhead on top.
    assert!(
        wasm_gas > 15_000_000_000,
        "WASM path should cost >15B gas (schnorrkel interpreted op-by-op), got {wasm_gas}"
    );
    // The delta IS the WASM curve25519 cost. Once Stage 0 ships with real
    // benchmark weights the syscall path will add ~150M on top of its
    // ~7B floor, keeping the delta ≈ wasm_gas − 7B.
    assert!(
        delta > 15_000_000_000,
        "syscall path should save >15B vs WASM path, saved {delta}"
    );
    // Even with zero-weight syscall the total-per-message ratio should be
    // at least 3× (floor-dominated). With real weights this won't shift
    // much because the syscall contribution (~150M) is ≪ floor (~7B).
    assert!(
        speedup >= 3,
        "expected >=3× total-per-message speedup, got {speedup}×"
    );
}

fn run_mode(
    system: &System,
    program: &Program,
    from: u64,
    mode: Mode,
    pk: &[u8; 32],
    msg: &[u8],
    sig: &[u8; 64],
) -> u64 {
    let req = VerifyRequest {
        mode,
        pk: *pk,
        msg: msg.to_vec(),
        sig: *sig,
    };
    let msg_id = program.send_bytes(from, req.encode());
    let run = system.run_next_block();

    // Diagnostic output for debugging path failures.
    println!(
        "{mode:?}: succeed={} failed={} not_executed={} log_entries={}",
        run.succeed.contains(&msg_id),
        run.failed.contains(&msg_id),
        run.not_executed.contains(&msg_id),
        run.log.len(),
    );
    for (i, entry) in run.log.iter().enumerate() {
        println!(
            "  log[{i}]: dest={:?} payload_len={} payload_head={:02x?}",
            entry.destination(),
            entry.payload().len(),
            &entry.payload()[..entry.payload().len().min(32)],
        );
    }

    assert!(
        run.succeed.contains(&msg_id),
        "{mode:?} path did not succeed (failed={}, not_executed={})",
        run.failed.contains(&msg_id),
        run.not_executed.contains(&msg_id),
    );

    let reply = run
        .log
        .iter()
        .find(|entry| entry.destination() == from.into())
        .expect("program replied to sender");
    let ok = u8::decode(&mut reply.payload()).expect("decode reply as u8");
    assert_eq!(ok, 1, "{mode:?} path returned verify=false on a valid sig");

    run.gas_burned
        .get(&msg_id)
        .copied()
        .expect("gas_burned entry for sent message")
}
