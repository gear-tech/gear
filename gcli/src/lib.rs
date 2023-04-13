// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! # gcli
//!
//! [![CI][ci1]][ci2]
//! [![docs][docs1]][docs2]
//! [![downloads][d1]][d2]
//! [![License][l1]][l2]
//!
//! [ci1]: https://github.com/gear-tech/gear/workflows/CI/badge.svg
//! [ci2]: https://github.com/gear-tech/gear/actions/workflows/CI.yaml
//!
//! [docs1]: https://img.shields.io/badge/current-docs-brightgreen.svg
//! [docs2]: https://docs.rs/gear-program/
//!
//! [d1]: https://img.shields.io/crates/d/gear-program.svg
//! [d2]: https://crates.io/crates/gear-program
//!
//! [l1]: https://img.shields.io/badge/License-GPL%203.0-success
//! [l2]: https://github.com/clearloop/gear-program/blob/master/LICENSE
//!
//!
//! ## Getting Started
//!
//! To install `gcli` via <kbd>cargo</kbd>
//!
//! ```sh
//! $ cargo install --git https://github.com/gear-tech/gear gcli
//! ```
//!
//! Usages:
//!
//! ```sh
//! $ gear
//! `gear` client cli
//!
//! Usage: gcli [OPTIONS] <COMMAND>
//!
//! Commands:
//!   claim           Claim value from mailbox
//!   create          Deploy program to gear node
//!   info            Get account info from ss58address
//!   key             Keypair utils
//!   login           Log in to account
//!   meta            Show metadata structure, read types from registry, etc
//!   new             Create a new gear program
//!   program         Read program state, etc
//!   reply           Sends a reply message
//!   send            Sends a message to a program or to another account
//!   upload          Saves program `code` in storage
//!   upload-program  Deploy program to gear node
//!   transfer        Transfer value
//!   update          Update self from crates.io or github
//!   help            Print this message or the help of the given subcommand(s)
//!
//! Options:
//!   -r, --retry <RETRY>        How many times we'll retry when RPC requests failed [default: 5]
//!   -v, --verbose              Enable verbose logs
//!   -e, --endpoint <ENDPOINT>  Gear node rpc endpoint
//!   -p, --passwd <PASSWD>      Password of the signer account
//!   -h, --help                 Print help
//! ```
//!
//! Now, let's create a <kbd>new</kbd> gear program and upload it to the staging testnet!
//!
//! ```sh
//! $ gear new hello-world
//! Cloning into '/home/clearloop/.gear/apps'...
//! remote: Enumerating objects: 156, done.
//! remote: Counting objects: 100% (156/156), done.
//! remote: Compressing objects: 100% (121/121), done.
//! remote: Total 156 (delta 41), reused 83 (delta 15), pack-reused 0
//! Receiving objects: 100% (156/156), 89.78 KiB | 723.00 KiB/s, done.
//! Resolving deltas: 100% (41/41), done.
//!  Successfully created registry at /home/clearloop/.gear/apps!
//!  Successfully created hello-world!
//! ```
//!
//! Compile you gear program via <kbd>cargo</kbd>
//!
//! ```sh
//! $ cargo build --manifest-path hello-world/Cargo.toml --release
//! ```
//!
//! <kbd>login</kbd> to your gear account
//!
//! ```sh
//! $ gear login //Alice
//!  Successfully logged in as 5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY!
//! ```
//!
//! <kbd>upload</kbd> your gear program
//!
//! ```sh
//! $ gear upload hello-world/target/wasm32-unknown-unknown/release/hello_world.wasm
//! [INFO ] Submited extrinsic Gear::upload_code
//! [INFO ]         Status: Ready
//! [INFO ]         Status: Broadcast( ["12D3KooWQbJXFeRDJqmLT6jqahsJpwKGL5xEJJ6F3tevR1R85Upz", "12D3KooWFwZEE7cVz7fPTUrekv2Xfv2sR5HMetpadw4W9fZnEBr5", "12D3KooWNmeoxqMTSc3CzeA5SLTQ6xYQo4yz3Az1zjnAqrhpmBSH", "12D3KooWLFN7AceaViuVDakKghwLVo9i91Bi8DLyf1BGg6ftVGnG", "12D3KooWJ9EASqU3T89z1EBYMnvfTh5WK4Rgw3RMensrx5STRvXR", "12D3KooWDuzvhmTAebZXGJG8SCurHkn9x6mmpiTSQygGoCvXmYmU", "12D3KooWH7QBPHh5Byc2ZBjGSiqBbGzBAr5E8mqLWueyPXQJrxWB", "12D3KooWRw1Yfdo86zpgN9TTLJ6J53iAM1y1PW9ogKHsTHvBPDg9", "12D3KooWJ15sMWcCgmSLBAfRD5TZgKoCCZ1xDzPRGzbR2YC5zKqS", "12D3KooWEMDPU47VnnZPLEMXeFJkphaG8kRdn9SuTqoJJEhrwC2w", "12D3KooWSfMsGDWG6hvTgfLoFETZrnxNC649bQwSa9FxTAPw4Cmy", "12D3KooWK7fw8MdENES5jAb8kjLw4b3eGMxuWBPR52v15FRkmYF3", "12D3KooWLP3AxJf1VfVJzbzcrHAkipXXa9bSvPcE1TowuRQZE8bz", "12D3KooWSf2d69w7RYKtj9mgYpLDs3rqLAz9GHNSHHoCQDLUjeiP", "12D3KooWSKMmTordwL3t6SkQaKXuXt1aYC2QZAXNyt8DxjpgFXYq", "12D3KooWEsvboSEFhf5utCZJ4gfUjb7S5i9Qec1TXB2DuYPJZVzB", "12D3KooWRf7vAr79yAyDxGvYAdSqhh2EoeWe35Lx4QH4N6XMv2gH", "12D3KooWPuaSwvwq2EGdasjJruUzR1wwTk1tDdVBZauKwG8ZPFi1", "12D3KooWHSepUMWdNVgKPhdquR12AzSZrkHwUsfXvVfFMPGXpyH5", "12D3KooWDC3qNpRz5LdSfPWi3XWfc7kG5GHyEDNR2NcgJMedfu5v", "12D3KooWRQ8oUwhrW84UuVpQNZ2QxS2kg3SyhLwVkwHHk9vJgf5q", "12D3KooWHZaCXaMgavJYoH925jiLrLhsbPpU14tt6M7ypenDyfPc", "12D3KooWAd4GWfAqNTqoqTNnjsKqJHWNRezgcHi742eGYKDdYsfC", "12D3KooWFWc6NFCiuTxd9iKq9mi1n3G7nBEZ5yDkzzHjkGBSceje", "12D3KooWQ8yjECbzLThEwzcTQ3gtVgZbb1XPBrPyHnRkmLJRGfEW", "12D3KooWFsZdJERxRrc5afrFDxvts4bDxSHHDgQxh8bTm4Kq9PV7", "12D3KooWGpxgFFTXij8gXzx6YgExaVczUN2fuohccrkA11tGFzDu", "12D3KooWND9qfwCVtfB17y9fcThBKoWvCSpXrCQCs6XsWvHE5om2", "12D3KooWLoCosNXv1HESuU76r7xmp5UU4pdCncnZXB1hYvcbCYgX", "12D3KooWEga7tssCYmywnRU492ANXV4vGYqX5AVJrrAAKQ1zhhGN", "12D3KooWDP1pb16iGikYc8fkkL8ZYmzPqsrVRzQHHBDKxjRpUMNA", "12D3KooWG26t3Z1NfeAPNWdwrdWYntSUj69LzHcnBdV4PcQMEuHA", "12D3KooWS5DUgYPSQVrexXbPksR4cVsexFhLXzXFgsY47ZPeFHd9", "12D3KooWBWFtZqigVTC8W2GRMwLeuTK2o4hDC4XHVPyNV6hW1T1D", "12D3KooWDCboxcE7VAB3v3UJf1hrNZiswyk5Eg1u2kaiSs4v6Sbi", "12D3KooWNx1mbmwKXSPS8vuHkyVrQrZnwp4HGjLczPxFCpAyRhNS", "12D3KooWJ3KhEHCm4roQw2LAUGu28fXJf5QqQHhG6EaACw6RCUjr", "12D3KooWFnr5yyEcNAfdjJjfuBAMaZ2iz3GLyFrJAs5AiRJ74vWS", "12D3KooWMNeo7UgreqFxQ6BstVgZrNAZMVyKt9EWnC6AD9J2M1rT", "12D3KooWEVvqVD2mrLfmgeX1EXZ2caFXXEWWEs4Taa4mWzFUoF34", "12D3KooWMadAihMmvZmGt1HpxGAqqjb7Q2q96VVev6rGA1GLuqjv", "12D3KooWARM6duzRRd64fMJZJY3VqWekeG1rmJqxxjNLRYaQVPRt", "12D3KooWSqCyNpmVwaAxS1mMms2GQvUcPzPdoWB2XjiWpXvGW3Jf", "12D3KooWH22kTRSvhRnMUtu7Eg8d96Ma68jzRKV7vtxVMwB96kvS", "12D3KooWN1LBk84vnJEsQ33WsPRvpSzfrNMUZ2iLhTkUYjsSfwR5", "12D3KooWMxtE2fWGZZsZjfjoRN5aH6ecSKj8YfTkufi2vtywoKLS", "12D3KooWGMpAqtwpGR4tcQ3tc2ZThkTUN2YYcgxQsuSbfdQ4h3E4", "12D3KooWSyBLw12Z8rHRx2NSAfmb3cpAP6nJ2qK5FkdEC38zNVKk", "12D3KooWH7sqE4cp9wyLt5Z7xzuqA2imNGMeUHnu2gPJ4hGnJqJv", "12D3KooWK896roWsGutzksP9cZc3oypVPjRB1o83uHzjxM72V7zb"] )
//! [INFO ]         Status: InBlock( block_hash: 0x4409…fa04, extrinsic_hash: 0x2c54…e9d9 )
//! [INFO ]         Status: Finalized( block_hash: 0x4409…fa04, extrinsic_hash: 0x2c54…e9d9 )
//! [INFO ] Successfully submited call Gear::upload_code 0x2c54…e9d9 at 0x4409…fa04!
//! [INFO ]         Balance spent: 3724868714
//! ```
//!
//! ## LICENSE
//!
//! GPL v3.0

pub mod cmd;
pub mod keystore;
pub mod meta;
pub mod result;
pub mod template;
pub mod utils;
