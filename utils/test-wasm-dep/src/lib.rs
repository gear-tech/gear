#[cfg(test)]
mod tests {
    mod binaries {
        include!(concat!(env!("OUT_DIR"), "/wasm_binaries.rs"));
    }

    #[test]
    fn it_works() {
        let code = binaries::demo_async_init::WASM_BINARY;
        println!(
            "demo-async-init: {:.2} MiB",
            code.len() as f32 / 1024.0 / 1024.0
        );

        let code = binaries::demo_async::WASM_BINARY;
        println!("demo-async: {:.2} MiB", code.len() as f32 / 1024.0 / 1024.0);
    }
}
