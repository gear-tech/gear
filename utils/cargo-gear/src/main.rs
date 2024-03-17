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
    io::Read,
    path::PathBuf,
    process,
    process::Command,
};

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

#[derive(Debug)]
struct RustcArgs {
    input: String,
    crate_name: Option<String>,
    crate_type: Vec<String>,
    target: Option<String>,
    cfg: HashMap<String, Vec<String>>,
}

impl RustcArgs {
    fn new(args: String) -> anyhow::Result<Self> {
        let args = args.split(' ');
        let mut parser = lexopt::Parser::from_iter(args);

        let mut input = None;
        let mut crate_name = None;
        let mut crate_type = vec![];
        let mut target = None;
        let mut cfg = HashMap::<String, Vec<String>>::new();

        while let Some(arg) = parser.next()? {
            match arg {
                Arg::Value(value) => {
                    input = Some(value.string()?);
                }
                Arg::Long("crate-name") => {
                    let value = parser.value()?.string()?;
                    crate_name = Some(value);
                }
                Arg::Long("crate-type") => {
                    let value = parser.value()?.string()?;
                    let value = value.split(',').map(str::to_string);
                    crate_type.extend(value);
                }
                Arg::Long("target") => {
                    let value = parser.value()?.string()?;
                    target = Some(value)
                }
                Arg::Long("cfg") => {
                    let value = parser.value()?.string()?;
                    let mut value = value.splitn(2, '=');
                    let key = value.next().expect("always Some").to_string();
                    let value = value
                        .next()
                        .map(|s| s.trim_matches('"'))
                        .map(str::to_string);
                    cfg.entry(key).or_default().extend(value);
                }
                // we don't care about other rustc flags
                Arg::Long(_) | Arg::Short(_) => {
                    let _ = parser.value();
                }
            }
        }

        Ok(Self {
            input: input.context("`INPUT` argument expected")?,
            crate_name,
            crate_type,
            target,
            cfg,
        })
    }
}

#[derive(Debug)]
struct CargoArgs {
    features: HashSet<String>,
    profile: Option<String>,
    release: bool,
    target_dir: Option<PathBuf>,
}

impl CargoArgs {
    fn from_env() -> anyhow::Result<Self> {
        let mut parser = lexopt::Parser::from_env();

        let mut features = HashSet::new();
        let mut profile = None;
        let mut release = false;
        let mut target_dir = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Arg::Short('F') | Arg::Long("features") => {
                    parser.values()?.try_fold(
                        &mut features,
                        |features, value| -> anyhow::Result<&mut _> {
                            let value = value.string()?;
                            features.extend(value.split(',').map(str::to_string));
                            Ok(features)
                        },
                    )?;
                }
                Arg::Long("profile") => {
                    let value = parser.value()?.string()?;
                    profile = Some(value);
                }
                Arg::Short('r') | Arg::Long("release") => {
                    release = true;
                }
                Arg::Long("target-dir") => {
                    let value: PathBuf = parser.value()?.parse()?;
                    target_dir = Some(value);
                }
                Arg::Value(_) => continue,
                // we don't care about other cargo flags
                Arg::Short(_) | Arg::Long(_) => {
                    let _ = parser.value()?;
                }
            }
        }

        anyhow::ensure!(
            !(release && profile.is_some()),
            "`--release` and `--profile` flags are mutually inclusive"
        );

        Ok(Self {
            features,
            profile,
            release,
            target_dir,
        })
    }

    fn profile(&self) -> String {
        if let Some(profile) = self.profile.clone() {
            profile
        } else if self.release {
            "release".to_string()
        } else {
            "dev".to_string()
        }
    }

    fn features(&self) -> &HashSet<String> {
        &self.features
    }

    fn target_dir(&self) -> Option<&PathBuf> {
        self.target_dir.as_ref()
    }
}

fn socket_name() -> String {
    let socket_name = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
    match local_socket::NameTypeSupport::query() {
        local_socket::NameTypeSupport::Both | local_socket::NameTypeSupport::OnlyNamespaced => {
            format!("@cargo-gear-{socket_name}")
        }
        local_socket::NameTypeSupport::OnlyPaths => env::temp_dir()
            .join(format!("cargo-gear-{socket_name}.sock"))
            .display()
            .to_string(),
    }
}

fn collect_rustc_args(
    socket_name: String,
    mut child: process::Child,
) -> anyhow::Result<Vec<RustcArgs>> {
    let listener = local_socket::LocalSocketListener::bind(socket_name)?;
    listener.set_nonblocking(true)?;

    let mut buf = vec![];

    let status = loop {
        let res = listener.accept();
        let mut stream = match res {
            Ok(stream) => stream,
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                if let Some(status) = child.try_wait()? {
                    break status;
                } else {
                    continue;
                }
            }
            err => err?,
        };
        stream.set_nonblocking(false)?;

        let mut content = String::new();
        stream.read_to_string(&mut content)?;
        println!("{}", content);
        let args = RustcArgs::new(content)?;
        buf.push(args);
    };
    anyhow::ensure!(status.success(), "WASM build failed");

    Ok(buf)
}

fn proxy_cargo_call() -> anyhow::Result<()> {
    let cargo = env::var("CARGO")?;
    let mut cargo = Command::new(cargo);
    cargo
        .args(env::args().skip(2)) // skip exe path and subcommand
        .arg("--features=tests-with-demos")
        .env("__GEAR_WASM_BUILT", "1");
    println!("{:?}", cargo);
    cargo.status()?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = CargoArgs::from_env()?;

    let metadata = cargo_metadata::MetadataCommand::new().no_deps().exec()?;

    let build_packages = BuildPackages::new(&metadata)?;

    let cargo_gear_dir = metadata.target_directory.join("cargo-gear");
    fs::create_dir_all(&cargo_gear_dir)?;
    let target_dir = cargo_gear_dir.join("target");

    let workspace_dir = workspace(cargo_gear_dir.clone().into_std_path_buf(), &build_packages)?;
    let socket_name = socket_name();

    let cargo = env::var("CARGO")?;
    let mut cargo = Command::new(cargo);
    cargo
        .arg("--config")
        .arg(r#"target.wasm32-unknown-unknown.rustflags=["-Clink-arg=--import-memory", "-Clinker-plugin-lto"]"#)
        .arg("build")
        .args(build_packages.cargo_args())
        .arg("--profile")
        .arg(args.profile())
        .current_dir(workspace_dir)
        .env("CARGO_BUILD_TARGET", "wasm32-unknown-unknown")
        .env("CARGO_TARGET_DIR", &target_dir)
        .env(
            "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
            "/Users/ark0f/CLionProjects/gear/target/debug/cargo-gear-rustc-wrapper",
        )
        .env("__CARGO_GEAR_SOCKET_NAME", &socket_name)
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
    let rustc_args = collect_rustc_args(socket_name, child)?;
    println!("{:#?}", rustc_args);

    for package in &build_packages.inner {
        let artifact_name = package.artifact_name();

        let rustc_called = rustc_args
            .iter()
            .flat_map(|args| args.crate_name.as_ref())
            .any(|crate_name| *crate_name == artifact_name);
        if !rustc_called {
            continue;
        }

        let wasm32_target_dir = target_dir
            .join("wasm32-unknown-unknown")
            .join(args.profile())
            .into_std_path_buf();
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

    proxy_cargo_call()?;

    Ok(())
}
