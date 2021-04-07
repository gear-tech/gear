//! Gear core.
//!
//! This library provides a runner for dealing with multiple little programs exchanging messages in a deterministic manner.
//! To be used primary in Gear Substrate node implementation, but it is not limited to that.

#![warn(missing_docs)]
#![cfg_attr(feature="strict", deny(warnings))]

pub mod env;
pub mod memory;
pub mod message;
pub mod program;
pub mod runner;
pub mod storage;

mod gas;
