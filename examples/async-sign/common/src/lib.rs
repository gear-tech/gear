#![no_std]

use codec::Decode;
use gstd::prelude::*;
use scale_info::TypeInfo;

#[derive(Debug, Decode, TypeInfo)]
pub struct HandleArgs {
    pub message: Vec<u8>,
    pub signature: Vec<u8>,
}
