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

use demo_crypto::Op;
use gtest::{Program, System, constants::DEFAULT_USER_ALICE};
use parity_scale_codec::Encode;
use sp_core::{Pair, sr25519};

/// Generate a random sr25519 keypair, sign a message, send the same
/// triple through both verify paths, and compare gas burns.
#[test]
fn sr25519_wasm_vs_syscall_gas_delta() {
    let system = System::new();
    system.init_logger();

    let (pair, _) = sr25519::Pair::generate();
    let pk: [u8; 32] = pair.public().0;
    let msg: Vec<u8> = b"gear-protocol-crypto-syscall-demo".to_vec();
    let sig: [u8; 64] = pair.sign(&msg).0;

    let program = Program::current(&system);
    let from = DEFAULT_USER_ALICE;

    // First send_bytes on a fresh program goes to init(), not handle().
    // Burn it on an empty init before the measured runs.
    let init_id = program.send_bytes(from, []);
    let init_run = system.run_next_block();
    assert!(
        init_run.succeed.contains(&init_id),
        "program init failed to succeed"
    );

    // sp_core's `Pair::sign` uses `b"substrate"` as the signing
    // context, so both paths must pass the same ctx for the sig to
    // validate. This is precisely the case the new ctx ABI exposes
    // to user programs — previously implicit, now explicit.
    let ctx: Vec<u8> = b"substrate".to_vec();

    let wasm_gas = run_verify(
        &system,
        &program,
        from,
        Op::Sr25519VerifyWasm {
            pk,
            ctx: ctx.clone(),
            msg: msg.clone(),
            sig,
        },
        "sr25519 WASM",
    );
    let sys_gas = run_verify(
        &system,
        &program,
        from,
        Op::Sr25519VerifySyscall {
            pk,
            ctx,
            msg: msg.clone(),
            sig,
        },
        "sr25519 syscall",
    );

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

    assert!(
        wasm_gas > 15_000_000_000,
        "WASM path should cost >15B gas (schnorrkel interpreted op-by-op), got {wasm_gas}"
    );
    assert!(
        delta > 15_000_000_000,
        "syscall path should save >15B vs WASM path, saved {delta}"
    );
    assert!(
        speedup >= 3,
        "expected >=3× total-per-message speedup, got {speedup}×"
    );
}

fn run_verify(system: &System, program: &Program, from: u64, op: Op, label: &str) -> u64 {
    let msg_id = program.send_bytes(from, op.encode());
    let run = system.run_next_block();

    assert!(
        run.succeed.contains(&msg_id),
        "{label} path did not succeed (failed={}, not_executed={})",
        run.failed.contains(&msg_id),
        run.not_executed.contains(&msg_id),
    );

    let reply = run
        .log
        .iter()
        .find(|entry| entry.destination() == from.into() && !entry.payload().is_empty())
        .expect("program replied to sender with a non-empty payload");
    assert_eq!(
        reply.payload(),
        &[1u8],
        "{label} path returned verify=false on a valid sig"
    );

    run.gas_burned
        .get(&msg_id)
        .copied()
        .expect("gas_burned entry for sent message")
}
