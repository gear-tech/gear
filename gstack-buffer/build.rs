use std::{env, error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let alloca_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("alloca");

    #[cfg(not(feature = "compile-alloca"))]
    if env::var("TARGET")? == "wasm32-unknown-unknown" {
        println!("cargo:rustc-link-lib=static=calloca");
        println!("cargo:rustc-link-search=native={}", alloca_dir.display());
    }

    #[cfg(feature = "compile-alloca")]
    {
        let mut builder = cc::Build::new();
        #[cfg(feature = "stack-clash-protection")]
        builder.flag_if_supported("-fstack-clash-protection");
        (if option_env!("CC") == Some("clang") {
            builder.flag("-flto")
        } else {
            &mut builder
        })
        .file(alloca_dir.join("alloca.c"))
        .opt_level(2)
        .compile("calloca");
    }

    Ok(())
}
