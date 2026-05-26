// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![allow(dead_code)]
#![doc(html_logo_url = "https://gear-tech.io/logo.png")]
#![doc(html_favicon_url = "https://gear-tech.io/favicon.ico")]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Vara.ETH SDK.

pub use crate::{api::VaraEthApi, mirror::Mirror, router::Router, wvara::WVara};

mod api;
mod mirror;
mod router;
mod wvara;

// Re-export the
pub use ethexe_node_wrapper::{Error, VaraEth, VaraEthInstance};
