//! command new
use crate::{Registry, Result};
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
    #[structopt(name = "TEMPLATE")]
    pub template: Option<String>,

    /// List avaiable templates
    #[structopt(short, long)]
    pub list: bool,
}

impl New {
    /// run command new
    pub fn exec(&self) -> Result<()> {
        let registry = Registry::default();
        let templates = templates(&registry)?;

        if let Some(template) = &self.template {
            if templates.contains(template) {
                copy_dir_all(&registry.path.join(&template), &template.into())?;
            }
            return Ok(());
        }

        println!("AVAIABLE TEMPLATES: \n\t{}", templates.join("\n\t"));

        Ok(())
    }
}

/// get all templates
fn templates(r: &Registry) -> Result<Vec<String>> {
    return Ok(fs::read_dir(&r.path)?
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
        .collect::<Vec<String>>());
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
