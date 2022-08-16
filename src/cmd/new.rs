//! command new
use crate::{registry, result::Result};
use std::{
    fs::{self, DirEntry},
    io,
    path::PathBuf,
};
use structopt::StructOpt;

/// Create a new gear program
#[derive(Debug, StructOpt)]
pub struct New {
    /// Create gear program from templates
    pub template: Option<String>,
}

impl New {
    /// run command new
    pub async fn exec(&self) -> Result<()> {
        registry::init().await?;
        let templates = templates()?;

        if let Some(template) = &self.template {
            if templates.contains(template) {
                copy_dir_all(&registry::GEAR_APPS_PATH.join(&template), &template.into())?;
            } else {
                crate::template::create(template)?;
            }

            println!("Successfully created {}!", template);
            return Ok(());
        }

        println!("AVAILABLE TEMPLATES: \n\t{}", templates.join("\n\t"));

        Ok(())
    }
}

/// get all templates
fn templates() -> Result<Vec<String>> {
    Ok(fs::read_dir(&*registry::GEAR_APPS_PATH)?
        .filter_map(|maybe_path: io::Result<DirEntry>| {
            if let Ok(p) = maybe_path {
                let path = p.path();
                if !path.is_dir() {
                    return None;
                }

                if let Some(file) = path.file_name() {
                    let name = file.to_string_lossy();
                    if !name.starts_with('.') {
                        return Some(name.into());
                    }
                }

                None
            } else {
                None
            }
        })
        .collect::<Vec<String>>())
}

/// copy -r
fn copy_dir_all(src: &PathBuf, dst: &PathBuf) -> Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            copy_dir_all(&path, &dst.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), &dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}
