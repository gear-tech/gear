use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir: PathBuf = env::var("CARGO_MANIFEST_DIR").unwrap().into();
    let workspace_dir = manifest_dir.ancestors().nth(2).unwrap();

    let content = fs::read_to_string(workspace_dir.join("rust-toolchain.toml")).unwrap();
    let line = content
        .lines()
        .find(|line| line.starts_with("channel"))
        .unwrap();
    let (start, end) = line.find('"').map(|i| i + 1).zip(line.rfind('"')).unwrap();
    let toolchain = &line[start..end];

    println!("cargo:rustc-env=WORKSPACE_TOOLCHAIN={toolchain}",);

    println!(
        "cargo:rustc-env=TARGET={}",
        std::env::var("TARGET").unwrap()
    );
}
