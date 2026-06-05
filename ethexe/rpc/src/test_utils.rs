// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

pub(crate) fn wasm_with_custom_sections(sections: &[(&str, &[u8])]) -> Vec<u8> {
    let mut wasm = b"\0asm\x01\0\0\0".to_vec();

    for (name, data) in sections {
        let section_len = 1 + name.len() + data.len();
        assert!(name.len() < 0x80);
        assert!(section_len < 0x80);

        wasm.push(0);
        wasm.push(section_len as u8);
        wasm.push(name.len() as u8);
        wasm.extend_from_slice(name.as_bytes());
        wasm.extend_from_slice(data);
    }

    wasm
}

pub(crate) fn wasm_with_custom_section(name: &str, data: &[u8]) -> Vec<u8> {
    wasm_with_custom_sections(&[(name, data)])
}
