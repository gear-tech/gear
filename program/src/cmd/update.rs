//! command `update`
use crate::result::Result;
use std::process::{self, Command};
use structopt::StructOpt;

const REPO: &str = "https://github.com/gear-tech/gear-program";

/// Update self from crates.io or github
#[derive(Debug, StructOpt)]
pub struct Update {
    /// Force update self from <https://github.com/gear-tech/gear-program>
    #[structopt(short, long)]
    pub force: bool,
}

impl Update {
    /// exec command update
    pub async fn exec(&self) -> Result<()> {
        let args: &[&str] = if self.force {
            &["--git", REPO, "--force"]
        } else {
            &[env!("CARGO_PKG_NAME")]
        };

        if !Command::new("cargo")
            .args([&["install"], args].concat())
            .status()?
            .success()
        {
            process::exit(1);
        }

        Ok(())
    }
}
