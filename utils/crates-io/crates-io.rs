//! Gear crates-io utils

use cargo_metadata::MetadataCommand;
use color_eyre::eyre::{eyre, Result};
use crates_io::Registry;
use curl::easy::Easy;

/// Pacakges have already taken by others.
const WHITELIST: [&str; 1] = ["gear-core-processor"];
const REGISTRY: &str = "https://crates.io";
const WIDTH: usize = 30;
const OWNERS: usize = 20;

/// Crates-io manager
struct CratesIo {
    packages: Vec<String>,
    registry: Registry,
}

impl CratesIo {
    /// Create a new crates-io manager.
    pub fn new() -> Result<Self> {
        let metadata = MetadataCommand::new().no_deps().exec()?;
        let packages = metadata
            .packages
            .into_iter()
            .filter(|p| !p.name.starts_with("demo"))
            .map(|p| p.name)
            .collect::<Vec<_>>();

        let mut handle = Easy::new();
        handle.useragent("crates-io-manager/0.0.0")?;

        let registry = Registry::new_handle(
            REGISTRY.into(),
            std::env::var("CRATES_IO_TOKEN").ok(),
            handle,
            false,
        );
        Ok(Self { packages, registry })
    }

    /// Get status of gear packages.
    pub fn status(&mut self) -> Result<()> {
        println!("{:0width$} | Owners", "Pacakge", width = WIDTH);
        println!("{} | {}", "-".repeat(WIDTH), "-".repeat(OWNERS));

        self.for_each_package(|package, owners| {
            println!("{:0width$} | {:?}", package, owners, width = 30,);

            Ok(())
        })
    }

    /// Run a function for each package.
    fn for_each_package(&mut self, f: impl Fn(&str, Vec<String>) -> Result<()>) -> Result<()> {
        for package in self.packages.iter() {
            if WHITELIST.contains(&package.as_str()) {
                continue;
            }

            match self.registry.list_owners(&package) {
                Ok(owners) => f(
                    package,
                    owners.into_iter().map(|u| u.login).collect::<Vec<_>>(),
                )?,
                Err(e) => {
                    if e.to_string().contains("404") {
                        println!("{:0width$} | no owner", package, width = WIDTH);
                    } else {
                        return Err(eyre!("{}: {}", package, e));
                    }
                }
            }
        }

        Ok(())
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;

    CratesIo::new()?.status()?;

    Ok(())
}
