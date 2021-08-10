use substrate_wasm_builder::WasmBuilder;

fn main() {
    // regular build
    WasmBuilder::new()
        .with_current_project()
        .export_heap_base()
        .import_memory()
        .build();
}
