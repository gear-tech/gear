use substrate_wasm_builder::WasmBuilder;

fn main() {
    // regular build
    WasmBuilder::new()
        .with_current_project()
        .import_memory()
        .build();
}
