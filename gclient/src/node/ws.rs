// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use std::{borrow::Cow, fmt};

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct WSAddress {
    domain: Cow<'static, str>,
    port: Option<u16>,
}

impl WSAddress {
    // Default substrate node port.
    const DEFAULT_PORT: u16 = 9944;

    // Local dev node.
    const LOCALHOST: &'static str = "ws://127.0.0.1";

    // Gear testnet.
    const GEAR: &'static str = "wss://rpc-node.gear-tech.io";
    const GEAR_PORT: u16 = 443;

    // Vara network.
    const VARA: &'static str = "wss://vara.gear.rs";

    pub fn new(domain: impl Into<Cow<'static, str>>, port: impl Into<Option<u16>>) -> Self {
        Self {
            domain: domain.into(),
            port: port.into(),
        }
    }

    pub fn dev() -> Self {
        Self::new(Self::LOCALHOST, Self::DEFAULT_PORT)
    }

    pub fn gear() -> Self {
        Self::new(Self::GEAR, Self::GEAR_PORT)
    }

    pub fn vara() -> Self {
        Self::new(Self::VARA, None)
    }

    pub fn url(&self) -> String {
        if let Some(port) = self.port {
            format!("{}:{port}", self.domain)
        } else {
            self.domain.to_string()
        }
    }
}

impl fmt::Debug for WSAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for WSAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.url())
    }
}
