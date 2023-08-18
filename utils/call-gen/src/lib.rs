// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Generator of the `pallet-gear` calls.

mod arbitrary_call;
mod claim_value;
mod create_program;
mod rand_utils;
mod send_message;
mod send_reply;
mod upload_code;
mod upload_program;

pub use arbitrary_call::GearCalls;
pub use claim_value::ClaimValueArgs;
pub use create_program::CreateProgramArgs;
pub use rand_utils::{CallGenRng, CallGenRngCore};
pub use send_message::SendMessageArgs;
pub use send_reply::SendReplyArgs;
pub use upload_code::UploadCodeArgs;
pub use upload_program::UploadProgramArgs;

pub(crate) use gear_wasm_gen::ConfigsBundle as GearWasmGenConfigsBundle;

#[derive(Debug, Clone, thiserror::Error)]
#[error("Can't convert to gear call {0:?} call")]
pub struct GearCallConversionError(pub &'static str);

pub type Seed = u64;

/// This trait must be implemented for all argument types
/// that are defined in [`GearCall`] variants.
pub trait CallArgs: GeneratableCallArgs + NamedCallArgs {}

impl<T: GeneratableCallArgs + NamedCallArgs> CallArgs for T {}

/// Describes type that can generate arguments of
/// the `gear` call.
///
/// Generates arguments for [`GearCall`] enum variants.
/// These arguments are later used to fuzz `gear` runtime
/// through `gear-node-loader` and `runtime-fuzzer`.
///
/// The trait is implemented for types, which can generate
/// arguments for this set of `gear` calls:
/// 1. `upload_program`
/// 2. `upload_code`
/// 3. `create_program`
/// 4. `send_message`
/// 5. `send_reply`
/// 6. `claim_value`
pub trait GeneratableCallArgs {
    /// Describes arguments of the test environment,
    /// which are written as a tuple of type `(T1, T2, T3, ...)`.
    ///
    /// Fuzzer args are those which are randomly generated
    /// by the PRNG before being passed to the `generate` method.
    type FuzzerArgs;
    /// Describes arguments of the test environment,
    /// that are taken from the it's configuration.
    type ConstArgs<C: GearWasmGenConfigsBundle>;

    /// Generates random arguments for [`GearCall`] variant.
    fn generate<Rng: CallGenRng, Config: GearWasmGenConfigsBundle>(
        _: Self::FuzzerArgs,
        _: Self::ConstArgs<Config>,
    ) -> Self;
}

/// Describes type that can tell for which `gear` call it carries arguments.
///
/// Intended to be implemented by the [`GeneratableCallArgs`] implementor.
pub trait NamedCallArgs {
    /// Returns name of the `gear` call for which `Self` carries arguments.
    fn name() -> &'static str;
}

/// Set of `pallet_gear` calls supported by the crate.
#[derive(Debug, Clone)]
pub enum GearCall {
    /// Upload program call args.
    UploadProgram(UploadProgramArgs),
    /// Send message call args.
    SendMessage(SendMessageArgs),
    /// Create program call args.
    CreateProgram(CreateProgramArgs),
    /// Upload program call args.
    UploadCode(UploadCodeArgs),
    /// Send reply call args.
    SendReply(SendReplyArgs),
    /// Claim value call args.
    ClaimValue(ClaimValueArgs),
}

#[macro_export]
macro_rules! impl_convert_traits {
    ($args:ty, $args_inner:ty, $gear_call_variant:ident, $gear_call_name:literal) => {
        impl From<$args> for $args_inner {
            fn from(args: $args) -> Self {
                args.0
            }
        }

        impl From<$args> for $crate::GearCall {
            fn from(args: $args) -> Self {
                $crate::GearCall::$gear_call_variant(args)
            }
        }

        impl TryFrom<$crate::GearCall> for $args {
            type Error = $crate::GearCallConversionError;

            fn try_from(call: $crate::GearCall) -> Result<Self, Self::Error> {
                if let $crate::GearCall::$gear_call_variant(call) = call {
                    Ok(call)
                } else {
                    Err($crate::GearCallConversionError($gear_call_name))
                }
            }
        }

        $crate::impl_named_call_args!($args, $gear_call_name);
    };
}

#[macro_export]
macro_rules! impl_named_call_args {
    ($args:tt, $gear_call_name:tt) => {
        impl $crate::NamedCallArgs for $args {
            fn name() -> &'static str {
                $gear_call_name
            }
        }
    };
}

/// Function generates WASM-binary of a Gear program with the
/// specified `seed`. `programs` may specify addresses which
/// can be used for send-calls.
pub fn generate_gear_program<Rng: CallGenRng, C: gear_wasm_gen::ConfigsBundle>(
    seed: Seed,
    config: C,
) -> Vec<u8> {
    use arbitrary::Unstructured;

    let mut rng = Rng::seed_from_u64(seed);

    let mut buf = vec![0; 100_000];
    rng.fill_bytes(&mut buf);

    let mut u = Unstructured::new(&buf);

    gear_wasm_gen::generate_gear_program_code(&mut u, config)
        .expect("failed generating gear program")
}
