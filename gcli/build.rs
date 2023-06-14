// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use std::env;
use which::which;

const LINUX_OPENSSL: &str = r#"
  It looks like you're compiling on Linux and also targeting Linux. Currently this
  requires the `pkg-config` utility to find OpenSSL but unfortunately `pkg-config`
  could not be found. If you have OpenSSL installed you can likely fix this by
  installing `pkg-config`.
"#;

// The term of this build script is for checking if pkg-config has
// been installed on the target machine since the downloading examples
// logic in gcli requires requesting github with native-tls.
fn main() {
    // NOTE: this is only a simple reminder for the Linux machines.
    // OSX and Windows users may not have this issue. Even if they
    // have, the panic message from `reqwest` will show the guides
    // in details anyway.
    if env::consts::OS == "linux" {
        if which("pkg-config").is_err() {
            panic!("{}", LINUX_OPENSSL);
        }
    }
}
