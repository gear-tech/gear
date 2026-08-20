// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Implementation of the `generate` subcommand
use crate::{
    utils::print_from_uri, with_crypto_scheme, CryptoSchemeFlag, Error, KeystoreParams,
    NetworkSchemeFlag, OutputTypeFlag,
};
use bip39::Mnemonic;
use clap::Parser;
use itertools::Itertools;

/// The `generate` command
#[derive(Debug, Clone, Parser)]
#[command(name = "generate", about = "Generate a random account")]
pub struct GenerateCmd {
    /// The number of words in the phrase to generate. One of 12 (default), 15, 18, 21 and 24.
    #[arg(short = 'w', long, value_name = "WORDS")]
    words: Option<usize>,

    #[allow(missing_docs)]
    #[clap(flatten)]
    pub keystore_params: KeystoreParams,

    #[allow(missing_docs)]
    #[clap(flatten)]
    pub network_scheme: NetworkSchemeFlag,

    #[allow(missing_docs)]
    #[clap(flatten)]
    pub output_scheme: OutputTypeFlag,

    #[allow(missing_docs)]
    #[clap(flatten)]
    pub crypto_scheme: CryptoSchemeFlag,
}

impl GenerateCmd {
    /// Run the command
    pub fn run(&self) -> Result<(), Error> {
        let words = match self.words {
            Some(words_count) if [12, 15, 18, 21, 24].contains(&words_count) => Ok(words_count),
            Some(_) => Err(Error::Input(
                "Invalid number of words given for phrase: must be 12/15/18/21/24".into(),
            )),
            None => Ok(12),
        }?;
        let mnemonic = Mnemonic::generate(words)
            .map_err(|e| Error::Input(format!("Mnemonic generation failed: {e}")))?;
        let password = self.keystore_params.read_password()?;
        let output = self.output_scheme.output_type;

        let phrase = mnemonic.words().join(" ");

        with_crypto_scheme!(
            self.crypto_scheme.scheme,
            print_from_uri(&phrase, password, self.network_scheme.network, output)
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate() {
        let generate = GenerateCmd::parse_from(["generate", "--password", "12345"]);
        assert!(generate.run().is_ok())
    }
}
