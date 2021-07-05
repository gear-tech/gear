//! Gear core.
//!
//! This library provides a runner for dealing with multiple little programs exchanging messages in a deterministic manner.
//! To be used primary in Gear Substrate node implementation, but it is not limited to that.
#![no_std]
#![warn(missing_docs)]
#![cfg_attr(feature = "strict", deny(warnings))]
#![doc(html_logo_url = "https://gear-tech.io/images/logo-black.svg")]

#[macro_use]
extern crate alloc;

pub mod env;
pub mod memory;
pub mod message;
pub mod program;
pub mod storage;

pub mod gas;
