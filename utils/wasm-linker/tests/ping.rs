use wasmparser::{Name, NameSectionReader, Parser, Payload};

#[test]
fn ping() {
    let ping = include_bytes!("../../../target/wasm32-unknown-unknown/release/demo_ping.wasm");

    for payload in Parser::new(0).parse_all(ping) {
        if let Ok(Payload::CustomSection(reader)) = payload {
            let mut nsr = NameSectionReader::new(reader.data(), reader.data_offset());
            while let Some(Ok(name)) = nsr.next() {
                if let Name::Function(name) = name {
                    for name in name.into_iter() {
                        println!("name: {:?}", name);
                    }
                }
            }
        }
    }
}
