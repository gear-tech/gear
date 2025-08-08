// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    process, thread, time,
};

use arbitrary::{Arbitrary as _, Unstructured};
use core_affinity::CoreId;
use ipc_channel::ipc::{IpcOneShotServer, IpcReceiver, IpcSender};
use lazy_pages_fuzzer::GeneratedModule;
use serde::{Deserialize, Serialize};

use crate::{
    generate_or_read_seed,
    seeds::{derivate_seed, generate_instance_seed},
    ts,
    utils::hex_to_string,
};

pub struct Worker {
    pub cpu_affinity: usize,
    pub process: process::Child,
    pub receiver: IpcReceiver<WorkerReport>,
    pub last_report: WorkerReport,
    pub exit_status: Option<process::ExitStatus>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerStatus {
    Started,
    Update { next_seed_to_fuzz: String },
}

impl WorkerStatus {
    pub fn seed(self) -> String {
        if let WorkerStatus::Update { next_seed_to_fuzz } = self {
            next_seed_to_fuzz
        } else {
            panic!("WorkerStatus is not Update");
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkerReport {
    pub pid: u32,
    pub status: WorkerStatus,
}

// Run a worker process that connects to the IPC server and performs fuzzing
pub fn run(token: String, ttl_sec: u64, cpu_affinity: usize) {
    if !core_affinity::set_for_current(CoreId { id: cpu_affinity }) {
        panic!("Failed to set CPU affinity {cpu_affinity} for worker process");
    }

    let pid = process::id();
    let tx: IpcSender<WorkerReport> = IpcSender::connect(token.to_string())
        .unwrap_or_else(|_| panic!("connect failed, pid {pid}"));

    tx.send(WorkerReport {
        pid: process::id(),
        status: WorkerStatus::Started,
    })
    .unwrap_or_else(|_| panic!("send failed, pid {pid}"));

    let input_seed = generate_or_read_seed(true);
    let now = time::Instant::now();

    while now.elapsed().as_secs() < ttl_sec {
        let instance_seed: [u8; 32] = generate_instance_seed(ts() + pid as u64);
        let derived_seed = derivate_seed(&input_seed, &instance_seed);

        if tx
            .send(WorkerReport {
                pid,
                status: WorkerStatus::Update {
                    next_seed_to_fuzz: hex_to_string(&instance_seed),
                },
            })
            .is_err()
        {
            // Main process has exited, worker should stop too
            break;
        }

        let mut u = Unstructured::new(&derived_seed);
        let m = GeneratedModule::arbitrary(&mut u).expect("Failed to generate module");
        let m = m.enhance().expect("cannot fail to enhance module");

        if lazy_pages_fuzzer::run(m).is_err() {
            panic!("failed to fuzz")
        }
    }
}

pub struct Workers {
    workers: HashMap<u32, Worker>,
    woker_ttl_sec: u64,
    executable_path: PathBuf,
}

impl Workers {
    pub fn spawn(ttl_sec: u64, num_workers: usize) -> Self {
        let executable_path = env::current_exe().expect("Failed to get current executable path");
        let mut workers = HashMap::new();

        for num in 0..num_workers {
            let (pid, worker) = Self::spawn_worker(&executable_path, ttl_sec, num);
            workers.insert(pid, worker);
        }

        Self {
            workers,
            woker_ttl_sec: ttl_sec,
            executable_path,
        }
    }

    fn spawn_worker(executable_path: &Path, ttl_sec: u64, cpu_affinity: usize) -> (u32, Worker) {
        let (server, token) =
            IpcOneShotServer::<WorkerReport>::new().expect("Failed to create IPC one-shot server.");

        let mut command = process::Command::new(executable_path);
        command.args([
            "worker",
            "--token",
            &token,
            "--ttl",
            &ttl_sec.to_string(),
            "--cpu-affinity",
            &cpu_affinity.to_string(),
        ]);

        let process = command.spawn().expect("Failed to start worker process");

        let (rx, msg) = server.accept().expect("accept failed");
        assert!(
            matches!(msg.status, WorkerStatus::Started),
            "Expected worker to start successfully"
        );
        let worker_pid = msg.pid;

        let worker = Worker {
            cpu_affinity,
            process,
            receiver: rx,
            last_report: msg,
            exit_status: None,
        };

        (worker_pid, worker)
    }

    pub fn run(&mut self, mut update_cb: impl FnMut()) -> Option<UserReport> {
        let mut exited_workers = Vec::new();

        loop {
            thread::sleep(time::Duration::from_millis(100));

            'outer: for (&pid, worker) in self.workers.iter_mut() {
                if let Some(exit_code) = worker.process.try_wait().ok().flatten() {
                    worker.exit_status = Some(exit_code);
                    exited_workers.push(pid);
                }

                loop {
                    if let Ok(report) = worker.receiver.try_recv() {
                        worker.last_report = report;
                        update_cb();
                    } else {
                        continue 'outer; // No more reports to process, continue to next worker
                    }
                }
            }

            // Remove && clean up workers that have exited
            for pid in exited_workers.iter() {
                if let Some(worker) = self.workers.remove(pid) {
                    let output = worker
                        .process
                        .wait_with_output()
                        .expect("Failed to wait for worker process");

                    if output.status.success() {
                        // Recreate exited worker
                        let (new_pid, new_worker) = Self::spawn_worker(
                            &self.executable_path,
                            self.woker_ttl_sec,
                            worker.cpu_affinity,
                        );
                        self.workers.insert(new_pid, new_worker);
                    } else {
                        return Some(UserReport {
                            instance_seed: worker.last_report.status.seed(),
                            pid: worker.last_report.pid,
                            exit_code: output.status.code().unwrap_or(1),
                            output,
                        });
                    }
                }
            }

            exited_workers.clear();
        }
    }
}

// NOTE: This dead code exists to silence warnings about unused struct fields.
// They're actually used in the sense that they're printed as user output in case of failure.
#[allow(dead_code)]
#[derive(Debug)]
pub struct UserReport {
    pub instance_seed: String,
    pub pid: u32,
    pub exit_code: i32,
    pub output: process::Output,
}
