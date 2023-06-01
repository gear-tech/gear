use super::seed;
use crate::{
    args::SeedVariant,
    batch_pool::{api::GearApiFacade, context::Context, Seed},
};
use anyhow::Result;
use codec::Encode;
use futures::FutureExt;
use gear_call_gen::{
    CallArgs, CallGenRng, CallGenRngCore, ClaimValueArgs, CreateProgramArgs, GearProgGenConfig,
    SendMessageArgs, SendReplyArgs, UploadCodeArgs, UploadProgramArgs,
};
use gear_core::ids::ProgramId;
use gear_utils::NonEmpty;
use std::{collections::HashMap, iter};
use tracing::instrument;

#[derive(Clone, Copy)]
pub struct RuntimeSettings {
    pub gas_limit: u64,
}

impl RuntimeSettings {
    pub async fn new(api: &GearApiFacade) -> Result<Self> {
        let gas_limit = api
            .raw_call(|gear_api| async { gear_api.block_gas_limit() }.boxed())
            .await?;

        Ok(Self { gas_limit })
    }
}

pub struct StressBatchGenerator<Rng> {
    pub batch_gen_rng: Rng,
    pub batch_size: usize,
    pub gas: u64,
    pub estimated: u128,
    pub pid_gas: HashMap<ProgramId, u64>,
    rt_settings: RuntimeSettings,
}

pub struct BatchGenerator<Rng> {
    pub batch_gen_rng: Rng,
    pub batch_size: usize,
    prog_gen_config: GearProgGenConfig,
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

        let prog_gen_config = GearProgGenConfig::new_normal();

        Self {
            batch_gen_rng,
            batch_size,
            prog_gen_config,
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
                let existing_programs = context.programs.iter().copied().collect::<Vec<_>>();
                Self::gen_batch::<UploadProgramArgs, _, _>(
                    self.batch_size,
                    seed,
                    |rng| {
                        (
                            existing_programs.clone(),
                            self.code_seed_gen.next_u64(),
                            rng.next_u64(),
                        )
                    },
                    || (rt_settings.gas_limit, self.prog_gen_config.clone()),
                )
            }
            1 => {
                let existing_programs = context.programs.iter().copied().collect::<Vec<_>>();
                Self::gen_batch::<UploadCodeArgs, _, _>(
                    self.batch_size,
                    seed,
                    |_| (existing_programs.clone(), self.code_seed_gen.next_u64()),
                    || (self.prog_gen_config.clone(),),
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
        ConstArgsFn: Fn() -> T::ConstArgs,
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
                .in_scope(|| T::generate::<Rng>(fuzzer_args, const_args_fn()))
            })
            .collect();

        inner.into()
    }
}

impl<Rng: CallGenRng> StressBatchGenerator<Rng> {
    pub fn new(
        seed: Seed,
        batch_size: usize,
        estimated: u128,
        rt_settings: RuntimeSettings,
    ) -> Self {
        let mut batch_gen_rng = Rng::seed_from_u64(seed);
        // tracing::info!("Code generator starts with seed: {code_seed_type:?}");

        Self {
            batch_gen_rng,
            batch_size,
            gas: 1,
            estimated,
            pid_gas: Default::default(),
            rt_settings,
        }
    }

    pub fn generate(&mut self, context: Context) -> BatchWithSeed {
        let seed = self.batch_gen_rng.next_u64();
        let batch_id = self.batch_gen_rng.gen_range(0..=1u8);
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
        let existing_programs = context.programs.iter().copied().collect::<Vec<_>>();
        if self.gas == 1 || context.programs.len() < self.batch_size {
            self.gen_upload_program_batch(existing_programs, seed, rt_settings)
        } else {
            match NonEmpty::from_vec(context.programs.iter().copied().collect()) {
                Some(existing_programs) => {
                    self.gen_send_message_batch(existing_programs, seed, rt_settings)
                }
                None => self.generate_batch(0, context, seed, rt_settings),
            }
        }
    }

    #[instrument(skip_all, fields(seed = seed, batch_type = "upload_program"))]
    fn gen_upload_program_batch(
        &mut self,
        existing_programs: Vec<ProgramId>,
        seed: Seed,
        rt_settings: RuntimeSettings,
    ) -> Batch {
        use demo_calc_hash_in_one_block::WASM_BINARY;

        let mut rng = Rng::seed_from_u64(seed);
        // let inner = utils::iterator_with_args(self.batch_size, || {
        //     (existing_programs.clone(), rng.next_u64())
        // })
        // .enumerate()
        // .map(|(i, (existing_programs, rng_seed))| {
        //     let mut rng = Rng::seed_from_u64(rng_seed);
        //     let mut salt = vec![0; rng.gen_range(1..=100)];
        //     rng.fill_bytes(&mut salt);
        //     UploadProgramArgs((WASM_BINARY.to_vec(), salt, vec![], rt_settings.gas_limit, 0))
        // })
        // .collect();

        let inner = iter::zip(
            1_usize..,
            iter::repeat_with(|| (existing_programs.clone(), seed)),
        )
        .take(self.batch_size)
        .map(|(i, (existing_programs, rng_seed))| {
            let mut salt = vec![0; rng.gen_range(1..=100)];
            rng.fill_bytes(&mut salt);
            UploadProgramArgs((WASM_BINARY.to_vec(), salt, vec![], rt_settings.gas_limit, 0))
        })
        .collect();

        Batch::UploadProgram(inner)
    }

    #[instrument(skip_all, fields(seed = seed, batch_type = "send_message"))]
    fn gen_send_message_batch(
        &mut self,
        existing_programs: NonEmpty<ProgramId>,
        seed: Seed,
        rt_settings: RuntimeSettings,
    ) -> Batch {
        use demo_calc_hash_in_one_block::Package;
        let inner = iter::zip(1_usize.., iter::repeat_with(|| existing_programs.clone()))
            .take(self.batch_size)
            .map(|(i, existing_programs)| {
                tracing::debug_span!("`stress send_message` generator", call_id = i + 1,).in_scope(
                    || {
                        // let program_idx = rng.next_u64() as usize;
                        tracing::debug!(
                            "{:?}, len = {}, {:?}",
                            existing_programs,
                            existing_programs.len(),
                            i
                        );
                        let &destination = existing_programs.get(i - 1).unwrap();
                        let src = [0; 32];
                        let payload = Package::new(self.estimated, src).encode();
                        SendMessageArgs((destination, payload, self.gas, 0))
                    },
                )
            })
            .collect();

        Batch::SendMessage(inner)
    }
}
