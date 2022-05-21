//! Examples registry
use crate::{Error, Result};
use std::{fs, path::PathBuf, process::Command};

/// gear-program examples' registry
pub struct Registry {
    /// https://github.com/gear-tech/apps.git by default
    pub repo: String,
    /// ~/.apps by default
    pub path: PathBuf,
}

impl Default for Registry {
    fn default() -> Self {
        let path = dirs::home_dir()
            .and_then(|mut p: PathBuf| {
                p.push(".gear/apps");
                Some(p)
            })
            .unwrap_or("./.gear/apps".into());

        Self {
            repo: "https://github.com/gear-tech/apps.git".into(),
            path,
        }
    }
}

impl Registry {
    /// Init registry
    pub async fn init(&self) -> Result<()> {
        if self.path.exists() {
            return Ok(());
        }

        // create home directory if not exists
        fs::create_dir_all(self.path.parent().ok_or(Error::CouldNotFindDirectory(
            self.path.to_string_lossy().into(),
        ))?)?;

        // clone registry repo into target
        Command::new("git")
            .args(&["clone", self.repo.as_ref(), &self.path.to_string_lossy()])
            .status()?;

        Ok(())
    }

    /// Update registry
    pub async fn update(&self) -> Result<()> {
        if !self.path.exists() {
            return self.init().await;
        }

        // update registry repo
        Command::new("git")
            .current_dir(&self.path)
            .args(&["pull"])
            .status()?;

        Ok(())
    }
}
