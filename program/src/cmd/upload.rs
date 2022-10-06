//! Command `upload`
use crate::{api::signer::Signer, result::Result};
use clap::Parser;
use std::{fs, path::PathBuf};

/// Saves program `code` in storage.
///
/// The extrinsic was created to provide _deploy program from program_ functionality.
/// Anyone who wants to define a "factory" logic in program should first store the code and metadata for the "child"
/// program in storage. So the code for the child will be initialized by program initialization request only if it exists in storage.
///
/// More precisely, the code and its metadata are actually saved in the storage under the hash of the `code`. The code hash is computed
/// as Blake256 hash. At the time of the call the `code` hash should not be in the storage. If it was stored previously, call will end up
/// with an `CodeAlreadyExists` error. In this case user can be sure, that he can actually use the hash of his program's code bytes to define
/// "program factory" logic in his program.
///
/// Parameters
/// - `code`: wasm code of a program as a byte vector.
///
/// Emits the following events:
/// - `SavedCode(H256)` - when the code is saved in storage.
#[derive(Parser, Debug)]
pub struct Upload {
    /// gear program code <*.wasm>
    code: PathBuf,
}

impl Upload {
    pub async fn exec(&self, signer: Signer) -> Result<()> {
        signer.upload_code(fs::read(&self.code)?).await?;

        Ok(())
    }
}
