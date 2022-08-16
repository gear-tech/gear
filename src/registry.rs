//! Examples registry
use crate::{
    result::{Error, Result},
    utils,
};
use lazy_static::lazy_static;
use std::{
    fs,
    path::PathBuf,
    process::{self, Command},
};

const GEAR_APPS: &str = "https://github.com/gear-tech/apps.git";

lazy_static! {
    /// registry path
    pub static ref GEAR_APPS_PATH: PathBuf = utils::home().join("apps");
}

/// Init registry
pub async fn init() -> Result<()> {
    if GEAR_APPS_PATH.exists() {
        return Ok(());
    }

    // create home directory if not exists
    let ps = GEAR_APPS_PATH.to_string_lossy();
    fs::create_dir_all(
        GEAR_APPS_PATH
            .parent()
            .ok_or_else(|| Error::CouldNotFindDirectory(ps.clone().into()))?,
    )?;

    // clone registry repo into target
    if !Command::new("git")
        .args(&["clone", GEAR_APPS, &ps, "--depth=1"])
        .status()?
        .success()
    {
        process::exit(1);
    }

    println!("Successfully created registry at {}!", &ps);
    Ok(())
}

/// Update registry
pub async fn update() -> Result<()> {
    if !GEAR_APPS_PATH.exists() {
        return init().await;
    }

    // update registry repo
    if !Command::new("git")
        .current_dir(&*GEAR_APPS_PATH)
        .args(&["pull"])
        .status()?
        .success()
    {
        process::exit(1);
    }

    Ok(())
}
