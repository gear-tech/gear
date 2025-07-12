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
    // NOTE: use vara here since vara includes all pallets gear have,
    // and the API we are building here is for both vara and gear.
    let [vara_runtime, api_gen] = [
        (VARA_RUNTIME_RELATIVE_PATH, VARA_RUNTIME_PKG, vec!["dev"]),
        (GSDK_API_GEN_RELATIVE_PATH, GSDK_API_GEN_PKG, vec![]),
    ]
    .map(|(relative_path, pkg, features)| get_path(relative_path, pkg, features));

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
fn get_path(relative_path: &str, pkg: &str, features: Vec<&'static str>) -> PathBuf {
    let out_dir: PathBuf = env::var("OUT_DIR")
        .expect("`OUT_DIR` is always set in build scripts")
        .into();

    let profile: String = out_dir
        .components()
        .rev()
        .take_while(|c| c.as_os_str() != "target")
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .take_while(|c| c.as_os_str() != "build")
        .last()
        .expect("Path should have subdirs in the `target` dir")
        .as_os_str()
        .to_string_lossy()
        .into();

    let target_dir = out_dir
        .ancestors()
        .find(|path| path.ends_with(&profile))
        .and_then(|path| path.parent())
        .expect("Could not find target directory");

    let path = PathBuf::from(format!(
        "{target_dir}/{profile}/{relative_path}",
        target_dir = target_dir.display()
    ));

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
