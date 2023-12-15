#[cfg(test)]
mod tests {
    mod binaries {
        include!(concat!(env!("OUT_DIR"), "/wasm_binaries.rs"));
    }

    #[test]
    fn it_works() {
        let code = binaries::demo_async::WASM_BINARY;
        println!("demo-async: {:.2} MB", code.len() as f32 / 1024.0 / 1024.00);

        let code = binaries::demo_async_custom_entry::WASM_BINARY;
        println!("demo-async: {:.2} MB", code.len() as f32 / 1024.0 / 1024.00);
    }
}
