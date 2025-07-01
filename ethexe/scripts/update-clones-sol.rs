#!/usr/bin/env rust-script

//! ```cargo
//! [dependencies]
//! anyhow = "1.0.98"
//! clap = { version = "^4.5.40", features = ["derive"] }
//! hex = "0.4.3"
//! serde = { version = "^1.0.219", features = ["derive"] }
//! serde_json = { version = "^1.0.140" }
//! ```

extern crate anyhow;
extern crate clap;
extern crate hex;
extern crate serde;
extern crate serde_json;

use anyhow::Result;
use clap::Parser;
use serde::Deserialize;
use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
};

#[derive(Debug, Parser)]
struct Cli {
    #[clap(short = 'm', long)]
    mirror_proxy: PathBuf,

    #[clap(short = 'c', long)]
    clones: PathBuf,
}

#[derive(Deserialize)]
struct BytecodeContent {
    object: String,
}

#[derive(Deserialize)]
struct SolidityBuildArtifact {
    bytecode: BytecodeContent,
}

const CLONES_CONTRACT_START: &[u8] = br#"
// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Memory} from "frost-secp256k1-evm/utils/Memory.sol";

library Clones {
    function cloneDeterministic(address router, bytes32 salt) internal returns (address instance) {
        return cloneDeterministic(router, salt, 0);
    }

    function cloneDeterministic(address router, bytes32 salt, uint256 value) internal returns (address instance) {
"#;

const CLONES_CONTRACT_END: &[u8] = br#"
        assembly ("memory-safe") {
            instance := create2(value, memPtr, size, salt)
            if iszero(instance) { revert(0x00, 0x00) }
        }
    }
}
"#;

const ROUTER_PLACEHOLDER: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const CHUNK_SIZE: usize = 32;
const ROUTER_PLACEHOLDER_LEN: usize = 20;
const INDENTATION: &str = "        ";

fn replace_placeholder_with_zeros(chunk: &[u8], start: usize, end: usize) -> String {
    let mut modified_chunk = chunk.to_vec();
    for i in start..end {
        modified_chunk[i] = 0;
    }

    hex::encode(&modified_chunk)
}

fn generate_to_file(mut file: File, bytecode: Vec<u8>) -> Result<()> {
    let bytecode_length = bytecode.len() as u16;
    let bytecode_length_bytes = hex::encode(bytecode_length.to_be_bytes());
    println!("Bytecode length: {bytecode_length}({bytecode_length_bytes})",);

    let router_placeholder =
        hex::decode(ROUTER_PLACEHOLDER).expect("Failed to decode router placeholder");

    let router_placeholder_index = bytecode
        .windows(20)
        .position(|window| window == router_placeholder)
        .unwrap();

    println!("Router placeholder index: {}\n", router_placeholder_index);

    file.write_all(CLONES_CONTRACT_START)
        .expect("Failed to write code");

    file.write_fmt(format_args!(
        "{}uint256 size = 0x{};\n",
        INDENTATION, &bytecode_length_bytes
    ))?;

    file.write_fmt(format_args!(
        "{}uint256 memPtr = Memory.allocate(size);\n\n",
        INDENTATION
    ))?;

    file.write(b"        /// @dev This bytecode is taken from `cat out/MirrorProxy.sol/MirrorProxy.json | jq -r \".bytecode.object\"`\n")?;

    for (i, chunk) in bytecode.chunks(CHUNK_SIZE).enumerate() {
        let offset = i * 32;
        let chunk_start = offset;
        let chunk_end = offset + CHUNK_SIZE;
        let placeholder_start = router_placeholder_index;
        let placeholder_end = router_placeholder_index + ROUTER_PLACEHOLDER_LEN;

        let offset_hex = hex::encode((offset as u16).to_be_bytes());

        let is_full_placeholder_in_chunk =
            chunk_start <= placeholder_start && chunk_end >= placeholder_end;
        let is_placeholder_start_in_chunk = chunk_start < placeholder_start
            && chunk_end > placeholder_start
            && chunk_end <= placeholder_end;
        let is_placeholder_end_in_chunk = chunk_start >= placeholder_start
            && chunk_start < placeholder_end
            && chunk_end > placeholder_end;

        // Check if the chunk contains the router placeholder
        if is_full_placeholder_in_chunk {
            let placeholder_start_in_chunk = placeholder_start - chunk_start;

            let modified_bytes_hex = replace_placeholder_with_zeros(
                chunk,
                placeholder_start_in_chunk,
                placeholder_start_in_chunk + ROUTER_PLACEHOLDER_LEN,
            );

            // Calculate shift for router address
            let shift_bits = (CHUNK_SIZE - ROUTER_PLACEHOLDER_LEN - placeholder_start_in_chunk) * 8;

            if shift_bits == 0 {
                file.write_fmt(format_args!(
                    "{}Memory.writeWord(memPtr, 0x{}, (0x{}) | (uint256(uint160(router))));\n",
                    INDENTATION, offset_hex, modified_bytes_hex
                ))?;
            } else {
                file.write_fmt(format_args!("{}Memory.writeWord(memPtr, 0x{}, (0x{}) | (uint256(uint160(router)) << {}));\n",
                    INDENTATION, offset_hex, modified_bytes_hex, shift_bits))?;
            }
        } else if is_placeholder_start_in_chunk {
            let placeholder_bytes_in_chunk = chunk_end - placeholder_start;
            let placeholder_start_in_chunk = placeholder_start - chunk_start;

            let modified_bytes_hex =
                replace_placeholder_with_zeros(chunk, placeholder_start_in_chunk, CHUNK_SIZE);

            let shift_right_bits = hex::encode(
                (((ROUTER_PLACEHOLDER_LEN - placeholder_bytes_in_chunk) * 8) as u8).to_be_bytes(),
            );

            file.write_fmt(format_args!(
                "{}Memory.writeWord(memPtr, 0x{}, (0x{}) | ((uint256(uint160(router)) >> 0x{})));\n",
                INDENTATION,
                offset_hex,
                modified_bytes_hex,
                shift_right_bits
            ))?;
        } else if is_placeholder_end_in_chunk {
            let placeholder_start_in_chunk = 0;
            let placeholder_bytes_in_chunk = placeholder_end - chunk_start;

            let modified_bytes_hex = replace_placeholder_with_zeros(
                chunk,
                placeholder_start_in_chunk,
                placeholder_bytes_in_chunk,
            );

            let bytes_in_chunk = (CHUNK_SIZE - placeholder_bytes_in_chunk) as u8;

            let shift_left_bits = hex::encode((bytes_in_chunk * 8u8).to_be_bytes());

            file.write_fmt(format_args!("{}Memory.writeWord(memPtr, 0x{}, (((uint256(uint160(router)) << 0x{}) | (0x{}))));\n",
                INDENTATION,
                offset_hex,
                shift_left_bits,
                modified_bytes_hex
            ))?;
        } else {
            let mut bytes_hex = hex::encode(chunk);
            if chunk.len() < CHUNK_SIZE {
                let zero_bytes = vec![0u8; CHUNK_SIZE - chunk.len()];
                let zero_bytes_hex = hex::encode(zero_bytes);
                bytes_hex.push_str(&zero_bytes_hex);
            }

            file.write_fmt(format_args!(
                "{}Memory.writeWord(memPtr, 0x{}, 0x{});\n",
                INDENTATION, offset_hex, bytes_hex
            ))?;
        }
    }

    file.write_all(CLONES_CONTRACT_END)?;
    Ok(())
}

fn main() {
    let args = Cli::parse();

    assert_eq!(
        args.mirror_proxy.extension().unwrap(),
        "json",
        "Invalid MirrorProxy build file. {}",
        args.mirror_proxy.display()
    );
    assert!(
        fs::exists(&args.mirror_proxy).unwrap(),
        "MirrorProxy build file not found"
    );
    assert_eq!(
        args.clones.extension().unwrap(),
        "sol",
        "Invalid Clones file path. {}",
        args.clones.display()
    );

    println!(
        "Mirror Proxy build artifact: {}",
        args.mirror_proxy.display()
    );

    let mirror_proxy_content =
        fs::read_to_string(&args.mirror_proxy).expect("Failed to read input file");

    let json_content: SolidityBuildArtifact =
        serde_json::from_str(&mirror_proxy_content).expect("Failed to parse JSON");

    let clones_contract = File::create(&args.clones).expect("Failed to create clones contract");

    let bytecode = hex::decode(
        json_content
            .bytecode
            .object
            .chars()
            .skip(2)
            .collect::<String>(),
    )
    .expect("Failed to parse bytecode");

    generate_to_file(clones_contract, bytecode).expect("Failed to write contract");

    println!("Clones contract: {}", args.clones.display());
}
