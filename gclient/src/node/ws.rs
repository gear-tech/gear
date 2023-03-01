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

/// Full WebSocket address required to specify the node.
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
    const VARA: &'static str = "wss://rpc.vara-network.io";

    /// Create a new `WSAddress` from a host `domain` and `port`.
    pub fn new(domain: impl Into<Cow<'static, str>>, port: impl Into<Option<u16>>) -> Self {
        Self {
            domain: domain.into(),
            port: port.into(),
        }
    }

    /// Return the address of the local node working in developer mode (running
    /// with `--dev` argument).
    ///
    /// # Examples
    ///
    /// ```
    /// use gclient::WSAddress;
    ///
    /// let address = WSAddress::dev();
    /// assert_eq!(address, WSAddress::new("ws://127.0.0.1", 9944));
    /// ```
    pub fn dev() -> Self {
        Self::dev_with_port(Self::DEFAULT_PORT)
    }

    /// Return the address of the local node working in developer mode (running
    /// with `--dev` argument).
    ///
    /// # Examples
    ///
    /// ```
    /// use gclient::WSAddress;
    ///
    /// let address = WSAddress::dev_with_port(1234);
    /// assert_eq!(address, WSAddress::new("ws://127.0.0.1", 1234));
    /// ```
    pub fn dev_with_port(port: u16) -> Self {
        Self::new(Self::LOCALHOST, port)
    }

    /// Return the default address of the public Gear testnet node.
    ///
    /// # Examples
    ///
    /// ```
    /// use gclient::WSAddress;
    ///
    /// let address = WSAddress::gear();
    /// assert_eq!(address, WSAddress::new("wss://rpc-node.gear-tech.io", 443));
    /// ```
    pub fn gear() -> Self {
        Self::new(Self::GEAR, Self::GEAR_PORT)
    }

    /// Return the default address of the public Vara node.
    ///
    /// # Examples
    ///
    /// ```
    /// use gclient::WSAddress;
    ///
    /// let address = WSAddress::vara();
    /// assert_eq!(address.url(), "wss://rpc.vara-network.io");
    /// ```
    pub fn vara() -> Self {
        Self::new(Self::VARA, None)
    }

    /// Convert the address to the URL string.
    ///
    /// # Examples
    ///
    /// ```
    /// use gclient::WSAddress;
    ///
    /// let address = WSAddress::new("wss://my-node.example.com", 443);
    /// assert_eq!(address.url(), "wss://my-node.example.com:443");
    /// ```
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
