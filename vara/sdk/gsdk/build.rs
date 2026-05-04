fn main() {
    // force subxt proc macro to use updated metadata
    println!("cargo:rerun-if-changed=vara_runtime.scale");
}
