// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::args::RustcArgs;
use interprocess::local_socket;
use rand::distributions::{Alphanumeric, DistString};
use std::{
    env, io,
    io::Read,
    process,
    process::{Command, ExitStatus},
};

pub const SOCKET_NAME_ENV: &str = "__CARGO_GEAR_SOCKET_NAME";

pub struct ArgsCollector {
    socket_name: String,
}

impl ArgsCollector {
    pub fn new() -> Self {
        Self {
            socket_name: Self::generate_socket_name(),
        }
    }

    fn generate_socket_name() -> String {
        let socket_name = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
        match local_socket::NameTypeSupport::query() {
            local_socket::NameTypeSupport::Both | local_socket::NameTypeSupport::OnlyNamespaced => {
                format!("@cargo-gear-{socket_name}")
            }
            local_socket::NameTypeSupport::OnlyPaths => env::temp_dir()
                .join(format!("cargo-gear-{socket_name}.sock"))
                .display()
                .to_string(),
        }
    }

    pub fn socket_name(&self) -> &String {
        &self.socket_name
    }

    pub fn collect(
        self,
        mut child: process::Child,
    ) -> anyhow::Result<(ExitStatus, Vec<RustcArgs>)> {
        let listener = local_socket::LocalSocketListener::bind(self.socket_name)?;
        listener.set_nonblocking(true)?;

        let mut buf = vec![];

        let status = loop {
            let res = listener.accept();
            let mut stream = match res {
                Ok(stream) => stream,
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    if let Some(status) = child.try_wait()? {
                        break status;
                    } else {
                        continue;
                    }
                }
                err => err?,
            };
            stream.set_nonblocking(false)?;

            let mut content = String::new();
            stream.read_to_string(&mut content)?;
            println!("{}", content);
            let args = RustcArgs::new(content)?;
            buf.push(args);
        };

        Ok((status, buf))
    }
}

pub fn main() {
    let mut args: Vec<String> = env::args().skip(1).collect();

    let name = env::var(SOCKET_NAME_ENV).unwrap();
    let mut stream = local_socket::LocalSocketStream::connect(name).unwrap();
    write!(&mut stream, "{}", args.join(" ")).unwrap();
    drop(stream);

    let rustc = args.remove(0);
    let status = Command::new(rustc).args(args).status().unwrap();
    assert!(status.success());
}
