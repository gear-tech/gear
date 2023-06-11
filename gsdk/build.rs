use std::{
    env, fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

const GSDK_API_GEN: &str = "GSDK_API_GEN";
const GSDK_API_GEN_PKG: &str = "gsdk-api-gen";
const GSDK_API_GEN_RELATIVE_PATH: &str = "gsdk-api-gen";
const VARA_RUNTIME_PKG: &str = "vara-runtime";
const VARA_RUNTIME_RELATIVE_PATH: &str = "wbuild/vara-runtime/vara_runtime.compact.compressed.wasm";
const GENERATED_API_PATH: &str = "src/metadata/generated.rs";
const ENV_RUNTIME_WASM: &str = "RUNTIME_WASM";

// These attributes are not supported by subxt 0.27.0.
//
// TODO: (issue #2666)
const INCOMPATIBLE_LINES: [&str; 4] = [
    ":: subxt :: ext :: scale_encode :: EncodeAsType ,",
    ":: subxt :: ext :: scale_decode :: DecodeAsType ,",
    r#"# [encode_as_type (crate_path = ":: subxt :: ext :: scale_encode")]"#,
    r#"# [decode_as_type (crate_path = ":: subxt :: ext :: scale_decode")]"#,
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=api-gen");
    println!("cargo:rerun-if-changed=../Cargo.lock");
    println!("cargo:rerun-if-changed=../runtime");
    println!("cargo:rerun-if-env-changed={}", GSDK_API_GEN);

    // This build script should only work when building gsdk as the primary package,
    // and the environment variable GSDK_API_GEN should be set to 1.
    if option_env!("CARGO_PRIMARY_PACKAGE") != Some("1") || env::var(GSDK_API_GEN) != Ok("1".into())
    {
        return;
    }

    let generated = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), GENERATED_API_PATH);
    fs::write(generated, generate_api()).expect("Failed to write generated api");
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
        (
            VARA_RUNTIME_RELATIVE_PATH,
            VARA_RUNTIME_PKG,
            vec!["debug-mode"],
        ),
        (GSDK_API_GEN_RELATIVE_PATH, GSDK_API_GEN_PKG, vec![]),
    ]
    .map(|(relative_path, pkg, features)| get_path(root, &profile, relative_path, pkg, features));

    // rebuild me.

    // Generate api
    let code = Command::new(api_gen)
        .env(ENV_RUNTIME_WASM, vara_runtime)
        .output()
        .expect("Failed to generate client api.")
        .stdout;

    format(&code).into_bytes()
}

// Format generated code with rustfmt.
//
// - remove the incompatible attributes.
// - remove verbose whitespaces.
fn format(stream: &[u8]) -> String {
    let mut raw = String::from_utf8_lossy(stream).to_string();
    for line in INCOMPATIBLE_LINES.iter() {
        raw = raw.replace(line, "");
    }

    let mut rustfmt = Command::new("rustfmt");
    let mut code = rustfmt
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Spawn rustfmt failed");

    code.stdin
        .as_mut()
        .expect("Get stdin of rustfmt failed")
        .write_all(raw.as_bytes())
        .expect("pipe generated code to rustfmt failed");

    let out = code.wait_with_output().expect("Run rustfmt failed").stdout;
    String::from_utf8_lossy(&out)
        .to_string()
        .replace(":: subxt", "::subxt")
        .replace(" :: ", "::")
        .replace("::subxt::utils::MultiAddress", "sp_runtime::MultiAddress")
        .replace("::subxt::utils::AccountId32", "sp_runtime::AccountId32")
}

// Get the path of the compiled package.
fn get_path(
    root: &str,
    profile: &str,
    relative_path: &str,
    pkg: &str,
    features: Vec<&'static str>,
) -> PathBuf {
    let path = PathBuf::from(format!("{}/../target/{}/{}", root, profile, relative_path));

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
            "package {} has not been compiled yet, please run \
             `cargo {}` first, or override environment `GEN_CLIENT_API` with `0` for disabling the api generation",
            pkg,
            args.join(" ")
        );
    }

    path
}
