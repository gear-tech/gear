use std::{env, fs, path::PathBuf, process::Command};

const GSDK_GEN_API: &'static str = "GSDK_GEN_API";
const GSDK_API_GEN_PKG: &'static str = "gsdk-api-gen";
const GSDK_API_GEN_RELATIVE_PATH: &'static str = "gsdk-api-gen";
const VARA_RUNTIME_PKG: &'static str = "vara-runtime";
const VARA_RUNTIME_RELATIVE_PATH: &'static str =
    "wbuild/vara-runtime/vara_runtime.compact.compressed.wasm";
const GENERATED_API_PATH: &'static str = "src/metadata/generated.rs";
const ENV_RUNTIME_WASM: &'static str = "RUNTIME_WASM";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../Cargo.lock");
    println!("cargo:rerun-if-changed=../runtime");
    println!("cargo:rerun-if-changed=../pallets/gear");
    println!("cargo:rerun-if-env-changed={}", GSDK_GEN_API);

    // This build script should only work when building gsdk as the primary package,
    // and the environment variable GSDK_GEN_API should be set to true.
    if env!("CARGO_PRIMARY_PACKAGE") != "1" || env::var(GSDK_GEN_API) != Ok("true".into()) {
        return;
    }

    let generated = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), GENERATED_API_PATH);
    fs::write(generated, generate_api()).expect("Failed to write generated api");
    post_fmt();
}

// Generate the node api for the client with the client-api-gen tool.
//
// # NOTE
//
// using an extra tool for doing this is for preventing the
// build-dependencies slow down the complation speed.
fn generate_api() -> Vec<u8> {
    let root = env!("CARGO_MANIFEST_DIR");
    let profile = env::var("PROFILE").expect("Environment PROFILE not found.");

    // NOTE: use vara here since vara includes all pallets gear have,
    // and the API we are building here is for both vara and gear.
    let [vara_runtime, api_gen] = [
        (VARA_RUNTIME_RELATIVE_PATH, VARA_RUNTIME_PKG),
        (GSDK_API_GEN_RELATIVE_PATH, GSDK_API_GEN_PKG),
    ]
    .map(|(relative_path, pkg)| get_path(root, &profile, relative_path, pkg));

    // Generate api
    Command::new(api_gen)
        .env(ENV_RUNTIME_WASM, vara_runtime)
        .output()
        .expect("Failed to generate client api.")
        .stdout
}

// Post format code since `cargo +nightly fmt` doesn't support pipe,
// not using `rustfmt` binary is beacuse our CI runs format check with
// the nightly version, but rustfmt could be stable version on our
// machines.
fn post_fmt() {
    let mut cargo = Command::new("cargo");
    cargo
        .args(["+nightly", "fmt", "-p", env!("CARGO_PKG_NAME")])
        .status()
        .expect("Format code failed.");
}

// Get the path of the compiled package.
fn get_path(root: &str, profile: &str, relative_path: &str, pkg: &str) -> PathBuf {
    let path = PathBuf::from(format!("{}/../target/{}/{}", root, profile, relative_path));

    // If package has not been compiled, compile it.
    if !path.exists() {
        let mut args = vec!["b", "-p", pkg];
        if profile == "release" {
            args.push("--release");
        }

        // NOTE: not gonna compile the package here since it may block the
        // build process.
        panic!(
            "package {} has not been compiled yet, please run \
             `cargo run {}` first, or set GEN_CLIENT_API=false \
             for the environment",
            pkg,
            args.join(" ")
        );
    }

    path
}
