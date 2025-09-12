// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

#![cfg_attr(not(feature = "std"), no_std)]

use gstd::Vec;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

#[cfg(feature = "std")]
mod code {
    include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));
}

#[cfg(feature = "std")]
pub use code::WASM_BINARY_OPT as WASM_BINARY;

#[cfg(not(feature = "std"))]
pub const WASM_BINARY: &[u8] = &[];

#[cfg(not(feature = "std"))]
pub mod wasm;

#[derive(Debug, Decode, Encode, TypeInfo)]
pub enum Alpn {
    Unsupported,
    HTTPv1_1,
    HTTPv2_0,
}

impl From<&[u8]> for Alpn {
    fn from(value: &[u8]) -> Self {
        match value {
            b"http/1.1" => Alpn::HTTPv1_1,
            b"h2" => Alpn::HTTPv2_0,
            _ => Alpn::Unsupported,
        }
    }
}

#[derive(Debug, Copy, Clone, Decode, Encode, TypeInfo)]
pub enum ProtocolVersion {
    Unsupported,
    TLSv1_2,
    TLSv1_3,
}

#[derive(Debug, Copy, Clone, Decode, Encode, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum CipherSuite {
    Unsupported,
    TLS_AES_256_GCM_SHA384,
    TLS_AES_128_GCM_SHA256,
    TLS_CHACHA20_POLY1305_SHA256,
}

#[derive(Debug, Copy, Clone, Decode, Encode, TypeInfo)]
pub enum Direction {
    Server,
    Client,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Decode, Encode, TypeInfo)]
pub enum ContentType {
    Unknown,
    ChangeCipherSpec,
    Alert,
    Handshake,
    ApplicationData,
    Heartbeat,
}

impl From<u8> for ContentType {
    fn from(value: u8) -> Self {
        match value {
            20 => ContentType::ChangeCipherSpec,
            21 => ContentType::Alert,
            22 => ContentType::Handshake,
            23 => ContentType::ApplicationData,
            24 => ContentType::Heartbeat,
            _ => ContentType::Unknown,
        }
    }
}

#[derive(Debug, Clone, Default, Decode, Encode, TypeInfo)]
pub struct NssKeylogValue {
    pub random: Vec<u8>,
    pub secret: Vec<u8>,
}

#[derive(Debug, Clone, Default, Decode, Encode, TypeInfo)]
pub struct NssKeylog {
    // TLS 1.2
    pub client_random: Option<NssKeylogValue>,
    // TLS 1.3
    pub client_handshake_traffic_secret: Option<NssKeylogValue>,
    pub server_handshake_traffic_secret: Option<NssKeylogValue>,
    pub client_traffic_secrets: Vec<NssKeylogValue>,
    pub server_traffic_secrets: Vec<NssKeylogValue>,
}

#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct TlsInfo {
    pub sni: Vec<u8>,
    pub alpn: Alpn,
    pub protocol_version: ProtocolVersion,
    pub cipher_suite: CipherSuite,
    pub peer_cert_chain: Vec<Vec<u8>>,
}

#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct TlsRecord {
    pub direction: Direction,
    pub content_type: ContentType,
    pub version: u16,
    pub length: u16,
    pub seq: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Decode, Encode, TypeInfo)]
pub struct Artifact {
    pub client_records: Vec<TlsRecord>,
    pub server_records: Vec<TlsRecord>,
    pub keylog: NssKeylog,
    pub info: TlsInfo,
}

/////

#[derive(Debug, Default, Copy, Clone, Decode, Encode, TypeInfo)]
pub struct Report {
    pub finished: FinishedReport,
    pub certs: CertReport,
    pub close: CloseReport,
}

#[derive(Debug, Default, Copy, Clone, Decode, Encode, TypeInfo)]
pub struct FinishedReport {
    pub server_finished_ok: bool,
    pub client_finished_ok: bool,
}

#[derive(Debug, Default, Copy, Clone, Decode, Encode, TypeInfo)]
pub struct CertReport {
    pub chain_valid: bool,
    pub cert_verify_valid: bool,
    pub dns_name_valid: bool,
}

#[derive(Debug, Default, Copy, Clone, Decode, Encode, TypeInfo)]
pub struct CloseReport {
    pub server_close_notify: bool,
    pub client_close_notify: bool,
    pub tcp_eof_observed: bool,
}
