// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use anyhow::Context;
use futures::{stream, StreamExt, TryStreamExt};
use gcli::cmd::config::Network;
use gear_core::{
    code::{Code, CodeError},
    gas_metering::Schedule,
};
use gprimitives::{CodeId, H256};
use gsdk::{metadata::storage::GearProgramStorage, Api};
use hex_literal::hex;
use std::{fs, future};
use tokio::task;
use tracing::Level;
use tracing_subscriber::EnvFilter;

const EXCLUDED_CODES: &[(Network, &[[u8; 32]])] = &[
    (Network::Mainnet, &[]),
    (
        Network::Testnet,
        &[
            // `gr_leave` somehow imported twice
            hex!("4dd9c141603a668127b98809742cf9f0819d591fe6f44eff63edf2b529a556bd"),
            hex!("90b021503f01db60d0ba00eac970d5d6845f1a757c667232615b5d6c0ff800cc"),
            // `init` entrypoint has invalid signature
            hex!("8990159f0730dfed622031af63c453d2bcd5644482cac651796bf229f25d23b6"),
            // `handle` entrypoint has invalid signature
            hex!("e8378f125ec82bb7f399d81b3481d5d62bb5d65749f47fea6cd65f7a48e9c24c"),
            hex!("d815332c3980386e58d0d191c5161d33824d8a6356a355ccb3528e6428551ab3"),
            // `init` export directly references `gr_leave` import
            hex!("ec0cc5d401606415c8ed31bfd347865d19fd277eec7d7bc62c164070eb8c241a"),
            // `gr_error` has been removed
            hex!("10d92d804fc4d42341d5eb2ca04b59e8534fd196621bd3908e1eda0a54f00ab9"),
            hex!("7ae2b90c96fd65439cd3c72d0c1de985b42400c5ad376d34d1a4cb070191ed2c"),
            // `delay` argument in `gr_reply` was removed
            hex!("2477bc4f927a3ae8c3534a824d6c5aec9fa9b0f4747a1f1d4ae5fabbe885b111"),
            hex!("7daa1b4f3a4891bda3c6b669ca896fa12b83ce4c4e840cf1d88d473a330c35fc"),
            // `gr_pay_program_rent` has been removed
            hex!("4a0bd89b42de7071a527c13ed52527e941dcda92578585e1139562cdf8a1063e"),
            hex!("d483a0e542ad20996b38a2efb1f41e8d863cc1659f1ceb89a79065849fadfeb5"),
            // `ext_logging_log_version_1` import somehow occurred
            hex!("75e61ed8f08379ff9ea7f69d542dceabf5f30bfcdf95db55eb6cab77ab3ddb56"),
            hex!("164dfe52b1438c7e38d010bc28efc85bd307128859d745e801c9099cbd82bd4f"),
            hex!("f92585a339751d7ba9da70a0536936cd8659df29bad777db13e1c7b813c1a301"),
            // `ext_misc_print_utf8_version_1` import somehow occurred
            hex!("c88b00cfd30d1668ebb50283b4785fd945ac36a4783f8eab39dec2819e06a6c9"),
        ],
    ),
];

struct InstrumentationResult {
    code_id: CodeId,
    original_code: Vec<u8>,
    res: Result<Code, CodeError>,
}

impl InstrumentationResult {
    async fn fetch_and_instrument(
        api: Api,
        latest_block: H256,
        code_id: CodeId,
    ) -> anyhow::Result<Self> {
        let original_code = api
            .original_code_storage_at(code_id, latest_block)
            .await
            .with_context(|| format!("Failed to fetch original code {code_id}"))?;

        let original_code_clone = original_code.clone();
        let res = task::spawn_blocking(move || {
            let schedule = Schedule::default();
            Code::try_new(
                original_code_clone,
                schedule.instruction_weights.version,
                |module| schedule.rules(module),
                schedule.limits.stack_height,
                schedule.limits.data_segments_amount.into(),
            )
        })
        .await
        .context("`gear_core::Code` panicked")?;

        Ok(Self {
            code_id,
            original_code,
            res,
        })
    }

    fn err(self) -> Option<(CodeId, Vec<u8>, CodeError)> {
        if let Err(err) = self.res {
            Some((self.code_id, self.original_code, err))
        } else {
            None
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(Level::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let network = Network::Testnet;
    let excluded_codes = EXCLUDED_CODES
        .iter()
        .find_map(|(n, codes)| (network == *n).then_some(*codes))
        .unwrap_or_default();
    let excluded_codes: Vec<CodeId> = excluded_codes
        .into_iter()
        .copied()
        .map(CodeId::new)
        .collect();

    let api = Api::new(Some(network.as_ref()))
        .await
        .context("Failed to create client")?;

    let latest_block = api
        .blocks()
        .at_latest()
        .await
        .context("Failed to get the latest block")?;
    let latest_block = latest_block.hash();

    let storage_key = Api::storage_root(GearProgramStorage::OriginalCodeStorage).to_root_bytes();
    let storage_key_len = storage_key.len();
    let storage = api.storage().at(latest_block);

    tracing::info!("Fetching all code IDs");
    let keys = storage
        .fetch_raw_keys(storage_key)
        .await
        .context("Failed to obtain stream of keys")?
        .map(|key| key.context("Failed to get key"))
        .and_then(|key| {
            future::ready(CodeId::try_from(&key[storage_key_len..]).context("Failed to parse key"))
        })
        .try_collect::<Vec<CodeId>>()
        .await?;
    let keys_len = keys.len();

    tracing::info!("Processing {keys_len} codes");
    let failed_codes = stream::iter(keys)
        .map(|code_id| {
            let api = api.clone();
            InstrumentationResult::fetch_and_instrument(api, latest_block, code_id)
        })
        .buffer_unordered(16)
        .chunks(100)
        .scan(0, |counter, chunk| {
            *counter += chunk.len();
            tracing::info!("[{counter}/{keys_len}] Instrumenting codes...");
            future::ready(Some(stream::iter(chunk)))
        })
        .flatten()
        .try_filter(|res| future::ready(!excluded_codes.contains(&res.code_id)))
        .try_filter_map(|res| future::ready(Ok(res.err())))
        .try_collect::<Vec<_>>()
        .await?;

    let temp_dir = tempfile::tempdir()
        .context("Failed to create temporary directory")?
        .into_path();

    if !failed_codes.is_empty() {
        tracing::warn!(
            dir = %temp_dir.display(),
            "There are {} failed codes. Their WASMs and WATs will be written",
            failed_codes.len(),
        );
    }

    for (code_id, original_code, err) in failed_codes {
        let mut wasm = temp_dir.join(code_id.to_string());
        wasm.set_extension("wasm");
        fs::write(&wasm, &original_code)
            .with_context(|| format!("Failed to write file {}", wasm.display()))?;

        let mut wat = wasm;
        wat.set_extension("wat");
        let wat_str =
            wasmprinter::print_bytes(original_code).context("Failed to convert WASM into WAT")?;
        fs::write(&wat, wat_str)
            .with_context(|| format!("Failed to write file {}", wat.display()))?;

        tracing::error!(%code_id, "{err}");
    }

    Ok(())
}
