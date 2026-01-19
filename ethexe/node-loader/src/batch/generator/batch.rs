use super::seed;
use crate::{args::SeedVariant, batch::context::Context, utils};
use anyhow::Result;
use ethexe_common::DEFAULT_BLOCK_GAS_LIMIT;
use futures::FutureExt;
use gear_call_gen::{
    CallArgs, CallGenRng, CallGenRngCore, ClaimValueArgs, CreateProgramArgs, Seed, SendMessageArgs,
    SendReplyArgs, UploadCodeArgs, UploadProgramArgs,
};
use gear_utils::NonEmpty;
use gear_wasm_gen::StandardGearWasmConfigsBundle;
use std::iter;
use tracing::instrument;

#[derive(Clone, Copy)]
pub struct RuntimeSettings {
    gas_limit: u64,
}

impl RuntimeSettings {
    pub fn new() -> Result<Self> {
        let gas_limit = DEFAULT_BLOCK_GAS_LIMIT;

        Ok(Self { gas_limit })
    }
}

pub struct BatchGenerator<Rng> {
    pub batch_gen_rng: Rng,
    pub batch_size: usize,
    code_seed_gen: Box<dyn CallGenRngCore>,
    rt_settings: RuntimeSettings,
}

// TODO #2202 Change to use GearCall
pub enum Batch {
    UploadProgram(Vec<UploadProgramArgs>),
    UploadCode(Vec<UploadCodeArgs>),
    SendMessage(Vec<SendMessageArgs>),
    CreateProgram(Vec<CreateProgramArgs>),
    SendReply(Vec<SendReplyArgs>),
    ClaimValue(Vec<ClaimValueArgs>),
}

macro_rules! impl_convert_for_batch {
    ($($args:ident $batch_variant:ident,)*) => {
        $(
            impl From<Vec<$args>> for Batch {
                fn from(vec: Vec<$args>) -> Self {
                    Self::$batch_variant(vec)
                }
            }
        )*
    };
}

impl_convert_for_batch![
    UploadProgramArgs UploadProgram,
    UploadCodeArgs UploadCode,
    SendMessageArgs SendMessage,
    CreateProgramArgs CreateProgram,
    SendReplyArgs SendReply,
    ClaimValueArgs ClaimValue,
];

pub struct BatchWithSeed {
    pub seed: Seed,
    pub batch: Batch,
}

impl BatchWithSeed {
    pub fn batch_str(&self) -> &'static str {
        match &self.batch {
            Batch::UploadProgram(_) => "upload_program",
            Batch::UploadCode(_) => "upload_code",
            Batch::SendMessage(_) => "send_message",
            Batch::CreateProgram(_) => "create_program",
            Batch::SendReply(_) => "send_reply",
            Batch::ClaimValue(_) => "claim_value",
        }
    }
}

impl From<BatchWithSeed> for Batch {
    fn from(other: BatchWithSeed) -> Self {
        other.batch
    }
}

impl From<(Seed, Batch)> for BatchWithSeed {
    fn from((seed, batch): (Seed, Batch)) -> Self {
        Self { seed, batch }
    }
}

impl From<BatchWithSeed> for (Seed, Batch) {
    fn from(BatchWithSeed { seed, batch }: BatchWithSeed) -> Self {
        (seed, batch)
    }
}

impl<Rng: CallGenRng> BatchGenerator<Rng> {
    pub fn new(
        seed: Seed,
        batch_size: usize,
        code_seed_type: Option<SeedVariant>,
        rt_settings: RuntimeSettings,
    ) -> Self {
        let mut batch_gen_rng = Rng::seed_from_u64(seed);
        let code_seed_type =
            code_seed_type.unwrap_or(SeedVariant::Dynamic(batch_gen_rng.next_u64()));

        tracing::info!("Code generator starts with seed: {code_seed_type:?}");

        Self {
            batch_gen_rng,
            batch_size,
            code_seed_gen: seed::some_generator::<Rng>(code_seed_type),
            rt_settings,
        }
    }

    pub fn generate(&mut self, context: Context) -> BatchWithSeed {
        let seed = self.batch_gen_rng.next_u64();
        let batch_id = self.batch_gen_rng.gen_range(0..=5u8);
        let rt_settings = self.rt_settings;

        let batch = self.generate_batch(batch_id, context, seed, rt_settings);

        (seed, batch).into()
    }

    fn generate_batch(
        &mut self,
        batch_id: u8,
        context: Context,
        seed: Seed,
        rt_settings: RuntimeSettings,
    ) -> Batch {
        match batch_id {
            0 => {
                let config = utils::get_wasm_gen_config(seed, context.programs.iter().copied());
                Self::gen_batch::<UploadProgramArgs, _, _>(
                    self.batch_size,
                    seed,
                    |rng| (self.code_seed_gen.next_u64(), rng.next_u64()),
                    || (rt_settings.gas_limit, config.clone()),
                )
            }
            1 => {
                let config = utils::get_wasm_gen_config(seed, context.programs.iter().copied());
                Self::gen_batch::<UploadCodeArgs, _, _>(
                    self.batch_size,
                    seed,
                    |_| self.code_seed_gen.next_u64(),
                    || (config.clone(),),
                )
            }
            2 => match NonEmpty::from_vec(context.programs.iter().copied().collect()) {
                Some(existing_programs) => Self::gen_batch::<SendMessageArgs, _, _>(
                    self.batch_size,
                    seed,
                    |rng| (existing_programs.clone(), rng.next_u64()),
                    || (rt_settings.gas_limit,),
                ),
                None => self.generate_batch(0, context, seed, rt_settings),
            },
            3 => match NonEmpty::from_vec(context.codes.iter().copied().collect()) {
                Some(existing_codes) => Self::gen_batch::<CreateProgramArgs, _, _>(
                    self.batch_size,
                    seed,
                    |rng| (existing_codes.clone(), rng.next_u64()),
                    || (rt_settings.gas_limit,),
                ),
                None => self.generate_batch(0, context, seed, rt_settings),
            },
            4 => match NonEmpty::from_vec(context.mailbox_state.iter().copied().collect()) {
                Some(mailbox_messages) => Self::gen_batch::<SendReplyArgs, _, _>(
                    self.batch_size,
                    seed,
                    |rng| (mailbox_messages.clone(), rng.next_u64()),
                    || (rt_settings.gas_limit,),
                ),
                None => self.generate_batch(0, context, seed, rt_settings),
            },
            5 => match NonEmpty::from_vec(context.mailbox_state.iter().copied().collect()) {
                Some(mailbox_messages) => Self::gen_batch::<ClaimValueArgs, _, _>(
                    self.batch_size,
                    seed,
                    |rng| (mailbox_messages.clone(), rng.next_u64()),
                    || (),
                ),
                None => self.generate_batch(0, context, seed, rt_settings),
            },
            _ => unreachable!(),
        }
    }

    #[instrument(skip_all, fields(seed = seed, batch_type = T::name()))]
    fn gen_batch<
        T: CallArgs,
        FuzzerArgsFn: FnMut(&mut Rng) -> T::FuzzerArgs,
        ConstArgsFn: Fn() -> T::ConstArgs<StandardGearWasmConfigsBundle>,
    >(
        batch_size: usize,
        seed: Seed,
        mut fuzzer_args_fn: FuzzerArgsFn,
        const_args_fn: ConstArgsFn,
    ) -> Batch
    where
        Batch: From<Vec<T>>,
    {
        let mut rng = Rng::seed_from_u64(seed);
        let inner: Vec<_> = iter::zip(1_usize.., iter::repeat_with(|| fuzzer_args_fn(&mut rng)))
            .take(batch_size)
            .map(|(i, fuzzer_args)| {
                tracing::debug_span!(
                    "gen_batch iteration",
                    generator_for = T::name(),
                    call_id = i
                )
                .in_scope(|| T::generate::<Rng, _>(fuzzer_args, const_args_fn()))
            })
            .collect();

        inner.into()
    }
}
