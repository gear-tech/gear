// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Gear Allocator.
//!
//! Lightweight memory allocator for Gear programs. Based on `dlmalloc` optimized fork.

#![no_std]
#![warn(missing_docs)]
#![doc(html_logo_url = "https://gear-tech.io/logo.png")]
#![doc(html_favicon_url = "https://gear-tech.io/favicon.ico")]
#![cfg_attr(docsrs, feature(doc_cfg))]

/// Global allocator instance.
// until https://github.com/alexcrichton/dlmalloc-rs/pull/26 is merged
#[cfg(not(windows))]
#[global_allocator]
pub static ALLOC: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

/// Prelude imports.
pub mod prelude;
