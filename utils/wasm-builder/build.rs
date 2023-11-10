use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir: PathBuf = env::var("CARGO_MANIFEST_DIR").unwrap().into();

    if let Some(rust_toolchain) = manifest_dir
        .ancestors()
        .nth(2)
        .map(|workspace_dir| workspace_dir.join("rust-toolchain.toml"))
        .filter(|path| path.exists())
    {
        let content = fs::read_to_string(rust_toolchain).unwrap();
        let line = content
            .lines()
            .find(|line| line.starts_with("channel"))
            .unwrap();
        let (start, end) = line.find('"').map(|i| i + 1).zip(line.rfind('"')).unwrap();
        let toolchain = &line[start..end];

        println!("cargo:rustc-env=WORKSPACE_TOOLCHAIN={toolchain}");
    }

    println!(
        "cargo:rustc-env=TARGET={}",
        std::env::var("TARGET").unwrap()
    );
}
