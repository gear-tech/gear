use crate::args::CargoArgs;
use anyhow::Context;
use cargo_metadata::Package;
use cargo_toml::Inheritable;
use gear_wasm_builder::{
    optimize,
    optimize::{OptType, Optimizer},
    smart_fs,
};
use interprocess::local_socket;
use lexopt::{Arg, ValueExt};
use rand::distributions::{Alphanumeric, DistString};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    env, fs, io,
    io::{Read, Write},
    path::PathBuf,
    process,
    process::Command,
};

mod args;
mod rustc_wrapper;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CargoMetadata {
    cargo_gear: Option<CargoGearMetadata>,
}

#[derive(Debug, Deserialize)]
struct CargoGearMetadata {}

#[derive(Debug)]
struct BuildPackages<'a> {
    inner: Vec<BuildPackage<'a>>,
}

impl<'a> BuildPackages<'a> {
    fn new(metadata: &'a cargo_metadata::Metadata) -> anyhow::Result<Self> {
        let mut packages = vec![];
        for package in metadata.workspace_packages() {
            let Some(metadata) =
                serde_json::from_value::<Option<CargoMetadata>>(package.metadata.clone())
                    .context("failed to parse custom metadata")?
                    .and_then(|metadata| metadata.cargo_gear)
            else {
                continue;
            };

            packages.push(BuildPackage {
                inner: package,
                metadata,
            });
        }

        Ok(Self { inner: packages })
    }

    fn cargo_args(&'a self) -> impl Iterator<Item = String> + 'a {
        self.inner.iter().flat_map(|package| package.cargo_args())
    }

    fn features(&'a self) -> impl Iterator<Item = &'a String> {
        self.inner.iter().flat_map(|package| package.features())
    }
}

#[derive(Debug)]
struct BuildPackage<'a> {
    inner: &'a Package,
    metadata: CargoGearMetadata,
}

impl<'a> BuildPackage<'a> {
    fn cargo_args(&self) -> impl IntoIterator<Item = String> {
        ["--package".to_string(), format!("{}-wasm", self.inner.name)]
    }
    fn features(&'a self) -> impl Iterator<Item = &'a String> {
        self.inner.features.keys()
    }

    fn artifact_name(&self) -> String {
        self.inner.name.replace('-', "_")
    }
}

fn workspace(cargo_gear_dir: PathBuf, packages: &BuildPackages) -> anyhow::Result<PathBuf> {
    const LIB_RS: &str = r#"
    #![no_std]
    #[allow(unused_imports)]
    pub use orig_project::*;
    "#;

    let workspace_dir = cargo_gear_dir.join("workspace");
    fs::create_dir_all(&workspace_dir)?;

    let root_manifest = cargo_toml::Manifest::<()> {
        workspace: Some(cargo_toml::Workspace {
            package: Some(cargo_toml::PackageTemplate {
                edition: Some(cargo_toml::Edition::E2021),
                version: Some("0.1.0".to_string()),
                ..Default::default()
            }),
            resolver: Some(cargo_toml::Resolver::V2),
            members: packages
                .inner
                .iter()
                .map(|package| format!("crates/{}", package.inner.name))
                .collect(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let root_manifest = toml::to_string_pretty(&root_manifest)?;
    smart_fs::write(workspace_dir.join("Cargo.toml"), root_manifest)?;

    let crates_dir = workspace_dir.join("crates");

    for package in &packages.inner {
        let mut toml_package = cargo_toml::Package::<()>::default();
        toml_package.name = format!("{}-wasm", package.inner.name);
        toml_package.edition = Inheritable::Inherited { workspace: true };
        toml_package.version = Inheritable::Inherited { workspace: true };

        let package_manifest = cargo_toml::Manifest {
            package: Some(toml_package),
            lib: Some(cargo_toml::Product {
                name: Some(package.artifact_name()),
                crate_type: vec!["cdylib".to_string()],
                ..Default::default()
            }),
            dependencies: [(
                "orig_project".to_string(),
                cargo_toml::Dependency::Detailed(Box::new(cargo_toml::DependencyDetail {
                    package: Some(package.inner.name.clone()),
                    path: Some(
                        package
                            .inner
                            .manifest_path
                            .join("..")
                            .canonicalize_utf8()?
                            .to_string(),
                    ),
                    default_features: false,
                    features: vec!["debug".to_string()],
                    ..Default::default()
                })),
            )]
            .into(),
            ..Default::default()
        };
        let package_manifest = toml::to_string_pretty(&package_manifest)?;
        let package_dir = crates_dir.join(&package.inner.name);
        fs::create_dir_all(&package_dir)?;
        smart_fs::write(package_dir.join("Cargo.toml"), package_manifest)?;

        let src_dir = package_dir.join("src");
        fs::create_dir_all(&src_dir)?;
        smart_fs::write(src_dir.join("lib.rs"), LIB_RS)?;
    }

    Ok(workspace_dir)
}

fn proxy_cargo_call(wasm32_target_dir: PathBuf) -> anyhow::Result<()> {
    let cargo = env::var("CARGO")?;
    let mut cargo = Command::new(cargo);
    cargo
        .args(env::args().skip(2)) // skip exe path and subcommand
        .env("__GEAR_WASM_BUILT", "1")
        .env("__GEAR_WASM_TARGET_DIR", wasm32_target_dir);
    println!("{:?}", cargo);
    cargo.status()?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    if env::var("__CARGO_GEAR_RUSTC_WRAPPER_MODE").as_deref() == Ok("1") {
        rustc_wrapper::main();
        return Ok(());
    }

    let args = CargoArgs::from_env()?;

    let metadata = cargo_metadata::MetadataCommand::new().no_deps().exec()?;

    let build_packages = BuildPackages::new(&metadata)?;

    let cargo_gear_dir = metadata.target_directory.join("cargo-gear");
    fs::create_dir_all(&cargo_gear_dir)?;
    let target_dir = cargo_gear_dir.join("target");

    let workspace_dir = workspace(cargo_gear_dir.clone().into_std_path_buf(), &build_packages)?;

    let rustc_args = rustc_wrapper::ArgsCollector::new();

    let cargo = env::var("CARGO")?;
    let mut cargo = Command::new(cargo);
    cargo
        .arg("--config")
        .arg(r#"target.wasm32-unknown-unknown.rustflags=["-Clink-arg=--import-memory", "-Clinker-plugin-lto"]"#)
        .arg("build")
        .args(build_packages.cargo_args())
        .arg("--profile")
        .arg(args.cargo_profile())
        .current_dir(workspace_dir)
        .env("CARGO_BUILD_TARGET", "wasm32-unknown-unknown")
        .env("CARGO_TARGET_DIR", &target_dir)
        .env(
            "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
            env::current_exe()?,
        )
        .env("__CARGO_GEAR_SOCKET_NAME", rustc_args.socket_name())
        .env("__CARGO_GEAR_RUSTC_WRAPPER_MODE", "1")
        .env("__GEAR_WASM_BUILDER_NO_BUILD", "1")
        .env("SKIP_WASM_BUILD", "1");

    let packages_features: HashSet<String> = build_packages.features().cloned().collect();
    let features: HashSet<&String> = packages_features.intersection(args.features()).collect();
    if !features.is_empty() {
        cargo.arg("--features").args(features);
    }

    if let Some(target_dir) = args.target_dir() {
        cargo.arg("--target-dir").arg(target_dir);
    }

    println!("{:?}", cargo);
    let child = cargo.spawn()?;
    let (status, rustc_args) = rustc_args.collect(child)?;
    anyhow::ensure!(status.success(), "WASM build failed");
    println!("{:#?}", rustc_args);

    let wasm32_target_dir = target_dir
        .join("wasm32-unknown-unknown")
        .join(args.dir_profile())
        .into_std_path_buf();

    for package in &build_packages.inner {
        let artifact_name = package.artifact_name();

        let rustc_called = rustc_args
            .iter()
            .flat_map(|args| args.crate_name.as_ref())
            .any(|crate_name| *crate_name == artifact_name);
        if !rustc_called {
            continue;
        }

        let wasm_bloaty = wasm32_target_dir.join(format!("{artifact_name}.wasm"));
        let wasm = wasm32_target_dir.join(format!("{artifact_name}.opt.wasm"));

        optimize::optimize_wasm(wasm_bloaty.clone(), wasm.clone(), "4", true).with_context(
            || {
                format!(
                    "failed to optimize {wasm_bloaty}",
                    wasm_bloaty = wasm_bloaty.display()
                )
            },
        )?;

        let mut optimizer = Optimizer::new(wasm.clone())?;
        optimizer.insert_stack_end_export().unwrap_or_else(|err| {
            println!(
                "Cannot insert stack end export into `{name}`: {err}",
                name = package.inner.name,
            )
        });
        optimizer.strip_custom_sections();

        let binary_opt = optimizer.optimize(OptType::Opt)?;
        fs::write(wasm, binary_opt)?;

        println!("{artifact_name} has been optimized");
    }

    proxy_cargo_call(wasm32_target_dir)?;

    Ok(())
}
