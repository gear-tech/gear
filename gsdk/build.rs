use gear_utils::codegen::format_with_rustfmt;
use std::{env, fs, path::PathBuf, process::Command};

const GSDK_API_GEN: &str = "GSDK_API_GEN";
const GSDK_API_GEN_PKG: &str = "gsdk-api-gen";
const GSDK_API_GEN_RELATIVE_PATH: &str = "gsdk-api-gen";
const VARA_RUNTIME_PKG: &str = "vara-runtime";
const VARA_RUNTIME_RELATIVE_PATH: &str = "wbuild/vara-runtime/vara_runtime.wasm";
const GENERATED_API_PATH: &str = "src/metadata/generated.rs";
const ENV_RUNTIME_WASM: &str = "RUNTIME_WASM";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed={GSDK_API_GEN}");

    // This build script should only work when building gsdk
    // with GSDK_API_GEN=1
    if env::var(GSDK_API_GEN) != Ok("1".into()) {
        return;
    }

    let generated = format!(
        "{}/{GENERATED_API_PATH}",
        env::var("CARGO_MANIFEST_DIR").unwrap()
    );
    fs::write(generated, generate_api()).expect("Failed to write generated api");
}

// Generate the node api for the client with the client-api-gen tool.
//
// # NOTE
//
// using an extra tool for doing this is for preventing the
// build-dependencies slow down the compilation speed.
fn generate_api() -> Vec<u8> {
    let root = env::var("CARGO_MANIFEST_DIR").expect("Environment CARGO_MANIFEST_DIR not found.");
    let profile = env::var("PROFILE").expect("Environment PROFILE not found.");

    // NOTE: use vara here since vara includes all pallets gear have,
    // and the API we are building here is for both vara and gear.
    let [vara_runtime, api_gen] = [
        (VARA_RUNTIME_RELATIVE_PATH, VARA_RUNTIME_PKG, vec!["dev"]),
        (GSDK_API_GEN_RELATIVE_PATH, GSDK_API_GEN_PKG, vec![]),
    ]
    .map(|(relative_path, pkg, features)| get_path(&root, &profile, relative_path, pkg, features));

    // Generate api
    let code = Command::new(api_gen)
        .env(ENV_RUNTIME_WASM, vara_runtime)
        .output()
        .expect("Failed to generate client api.")
        .stdout;

    // Remove the incompatible attributes and verbose whitespaces.
    format_with_rustfmt(&code)
        .replace(":: subxt", "::subxt")
        .replace(" : :: ", ": ::")
        .replace(" :: ", "::")
        .into_bytes()
}

// Get the path of the compiled package.
fn get_path(
    root: &str,
    profile: &str,
    relative_path: &str,
    pkg: &str,
    features: Vec<&'static str>,
) -> PathBuf {
    let path = PathBuf::from(format!("{root}/../target/{profile}/{relative_path}"));

    // If package has not been compiled, compile it.
    if !path.exists() {
        let mut args = ["build", "--package", pkg]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();

        if !features.is_empty() {
            args.push("--features".into());
            args.push(features.join(","));
        }

        if profile == "release" {
            args.push("--release".into());
        }

        // NOTE: not gonna compile the package here since it may block the
        // build process.
        panic!(
            "package {pkg} has not been compiled yet, please run \
             `cargo {}` first, or override environment `GEN_CLIENT_API` with `0` for disabling the api generation",
            args.join(" ")
        );
    }

    path
}
