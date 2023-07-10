//! Gear crates-io utils

use cargo_metadata::MetadataCommand;
use clap::{Parser, Subcommand};
use color_eyre::eyre::{eyre, Result};
use crates_io::Registry;
use curl::easy::Easy;
use std::{fs, path::PathBuf, process::Command};

/// Pacakges have already taken by others.
const WHITELIST: [&str; 1] = ["gear-core-processor"];
const REGISTRY: &str = "https://crates.io";
const WIDTH: usize = 30;
const OWNERS: usize = 50;
const DEV_TEAM_LOGIN: &str = "github:gear-tech:dev";
const DUMMY_PACKAGE: &str = "gear-package-template/Cargo.toml";
const DUMMY_PACKAGE_NAME: &str = r#"name = "dummy""#;

/// Crates-io manager
struct CratesIo {
    packages: Vec<String>,
    registry: Registry,
}

impl CratesIo {
    /// Create a new crates-io manager.
    pub fn new(token: &str) -> Result<Self> {
        let metadata = MetadataCommand::new().no_deps().exec()?;
        let packages = metadata
            .packages
            .into_iter()
            .filter(|p| !p.name.starts_with("demo"))
            .map(|p| p.name)
            .collect::<Vec<_>>();

        let mut handle = Easy::new();
        handle.useragent("crates-io-manager/0.0.0")?;

        let registry = Registry::new_handle(REGISTRY.into(), Some(token.into()), handle, false);
        Ok(Self { packages, registry })
    }

    /// Publish all unpublished packages.
    pub fn publish(&mut self) -> Result<()> {
        self.for_each_package(|_, package, owners| {
            let Err(e) = owners else {
                return Ok(())
            };

            if !e.to_string().contains("404") {
                return Ok(());
            }

            let path = PathBuf::from(DUMMY_PACKAGE);
            let manifest = fs::read_to_string(&path)?;

            // Change package name.
            let new_name = format!("name = \"{}\"", package);
            fs::write(&path, manifest.replace(&DUMMY_PACKAGE_NAME, &new_name))?;

            let revert_package_name = || -> Result<()> {
                let manifest = fs::read_to_string(&path)?.replace(&new_name, &DUMMY_PACKAGE_NAME);
                fs::write(&path, manifest)?;

                Ok(())
            };

            // Publish the crate.
            println!("publishing {}...", package);
            let status = Command::new("cargo")
                .arg("publish")
                .arg("--manifest-path")
                .arg(DUMMY_PACKAGE)
                .arg("--allow-dirty")
                .status()?;

            if status.success() {
                println!("published {}!", package);
            } else {
                revert_package_name()?;
                return Err(eyre!("Failed to publish {}", package));
            }

            // Add dev team as owner.
            println!("adding {} as owner of {}...", DEV_TEAM_LOGIN, package);
            let status = Command::new("cargo")
                .current_dir(path.parent().ok_or(eyre!("no parent"))?)
                .arg("owner")
                .arg("--add")
                .arg(DEV_TEAM_LOGIN)
                .status()?;

            if status.success() {
                println!("Added github:gear-tech:dev to {}!", package);
            } else {
                revert_package_name()?;
                return Err(eyre!(
                    "Failed to add github:gear-tech:dev as owner of {}",
                    package
                ));
            }

            revert_package_name()?;
            Ok(())
        })
    }

    /// Get status of gear packages.
    pub fn status(&mut self) -> Result<()> {
        println!("{:0width$} | Owners", "Pacakge", width = WIDTH);
        println!("{} | {}", "-".repeat(WIDTH), "-".repeat(OWNERS));

        self.for_each_package(|_, package, owners| {
            match owners {
                Ok(owners) => {
                    println!("{:0width$} | {}", package, owners.join(", "), width = WIDTH)
                }
                Err(e) => {
                    if e.to_string().contains("404") {
                        println!("{:0width$} | no owner", package, width = WIDTH);
                    } else {
                        return Err(eyre!("{}: {}", package, e));
                    }
                }
            }

            Ok(())
        })
    }

    /// Add owner for all packages.
    pub fn add_owner(&mut self, login: String) -> Result<()> {
        self.for_each_package(|registry, package, owners| {
            let Ok(owners) = owners else {
                return Ok(())
            };

            if !owners.contains(&login) {
                println!("adding shamil as owner of {}...", package);
                registry
                    .add_owners(&package, &[&login])
                    .map_err(|e| eyre!(e))?;
            }

            Ok(())
        })
    }

    /// Remove owner for all packages.
    pub fn remove_owner(&mut self, login: String) -> Result<()> {
        self.for_each_package(|registry, package, owners| {
            let Ok(owners) = owners else {
                return Ok(());
            };

            if owners.contains(&login) {
                println!("removing clearloop as owner of {}...", package);
                registry
                    .remove_owners(&package, &[&login])
                    .map_err(|e| eyre!(e))?;
            }

            Ok(())
        })
    }

    /// Run a function for each package.
    fn for_each_package(
        &mut self,
        f: impl Fn(&mut Registry, &str, Result<Vec<String>>) -> Result<()>,
    ) -> Result<()> {
        for package in self.packages.iter() {
            if WHITELIST.contains(&package.as_str()) {
                continue;
            }

            let owners = self
                .registry
                .list_owners(&package)
                .map(|owners| {
                    owners
                        .into_iter()
                        .map(|u| u.login.clone())
                        .collect::<Vec<_>>()
                })
                .map_err(|e| eyre!(e));

            f(&mut self.registry, package, owners)?;
        }

        Ok(())
    }
}

#[derive(Parser)]
struct Opt {
    /// Crates-io token.
    #[clap(required = true, short, long)]
    pub token: String,

    /// Subcommands.
    #[clap(subcommand)]
    pub command: SubCommand,
}

/// Crates-io commands.
#[derive(Subcommand)]
enum SubCommand {
    /// Show crates-io status.
    Status,
    /// Publish crates-io packages.
    Publish,
    /// Add owner for all packages.
    AddOwner { login: String },
    /// Remove owner for all packages.
    RemoveOwner { login: String },
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let opt = Opt::parse();
    let mut manager = CratesIo::new(&opt.token)?;
    match opt.command {
        SubCommand::Status => manager.status()?,
        SubCommand::Publish => manager.publish()?,
        SubCommand::AddOwner { login } => manager.add_owner(login)?,
        SubCommand::RemoveOwner { login } => manager.remove_owner(login)?,
    }

    Ok(())
}
