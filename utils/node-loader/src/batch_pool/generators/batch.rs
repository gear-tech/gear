use super::seed;
use crate::{
    args::SeedVariant,
    batch_pool::{
        api::GearApiFacade,
        batch::{
            Batch, BatchWithSeed, CreateProgramArgs, SendMessageArgs, UploadCodeArgs,
            UploadProgramArgs,
        },
        context::Context,
        Seed,
    },
    utils::{self, LoaderRng, LoaderRngCore, NonEmptyVec},
};
use anyhow::Result;
use futures::FutureExt;
use gear_core::ids::{CodeId, ProgramId};
use tracing::instrument;

#[derive(Clone, Copy)]
pub struct RuntimeSettings {
    gas_limit: u64,
}

impl RuntimeSettings {
    pub async fn new(api: &GearApiFacade) -> Result<Self> {
        let gas_limit = api
            .raw_call(|gear_api| async { gear_api.block_gas_limit() }.boxed())
            .await?;

        Ok(Self { gas_limit })
    }
}

pub struct BatchGenerator<Rng: LoaderRng> {
    pub batch_gen_rng: Rng,
    pub batch_size: usize,
    code_seed_gen: Box<dyn LoaderRngCore>,
    rt_settings: RuntimeSettings,
}

impl<Rng: LoaderRng> BatchGenerator<Rng> {
    pub fn new(
        seed: Seed,
        batch_size: usize,
        code_seed_type: Option<SeedVariant>,
        rt_settings: RuntimeSettings,
    ) -> Self {
        Self {
            batch_gen_rng: Rng::seed_from_u64(seed),
            batch_size,
            code_seed_gen: seed::some_generator::<Rng>(code_seed_type),
            rt_settings,
        }
    }

    pub fn generate(&mut self, context: Context) -> BatchWithSeed {
        let seed = self.batch_gen_rng.next_u64();
        let spec = self.batch_gen_rng.gen_range(0..=3u8);
        let rt_settings = self.rt_settings;

        let batch = match spec {
            0 => self.gen_upload_program_batch(seed, rt_settings),
            1 => {
                let span = tracing::debug_span!(
                    "gen_upload_code_batch",
                    seed = seed,
                    batch_type = "upload_code"
                );
                span.in_scope(|| self.gen_upload_code_batch())
            }
            2 => match NonEmptyVec::try_from_iter(context.programs.iter().copied()) {
                Ok(existing_programs) => {
                    self.gen_send_message_batch(existing_programs, seed, rt_settings)
                }
                Err(_) => self.gen_upload_program_batch(seed, rt_settings),
            },
            3 => match NonEmptyVec::try_from_iter(context.codes.iter().copied()) {
                Ok(existing_codes) => {
                    self.gen_create_program_batch(existing_codes, seed, rt_settings)
                }
                Err(_) => self.gen_upload_program_batch(seed, rt_settings),
            },
            _ => unreachable!(),
        };

        (seed, batch).into()
    }

    #[instrument(skip_all, fields(seed = seed, batch_type = "upload_program"))]
    fn gen_upload_program_batch(&mut self, seed: Seed, rt_settings: RuntimeSettings) -> Batch {
        let mut rng = Rng::seed_from_u64(seed);
        let inner = utils::iterator_with_args(self.batch_size, || {
            (self.code_seed_gen.next_u64(), rng.next_u64())
        })
        .enumerate()
        .map(|(i, (code_seed, rng_seed))| {
            tracing::debug_span!("`upload_program` generator", call_id = i + 1).in_scope(|| {
                UploadProgramArgs::generate::<Rng>(code_seed, rng_seed, rt_settings.gas_limit)
            })
        })
        .collect();

        Batch::UploadProgram(inner)
    }

    fn gen_upload_code_batch(&mut self) -> Batch {
        let inner = utils::iterator_with_args(self.batch_size, || self.code_seed_gen.next_u64())
            .enumerate()
            .map(|(i, code_seed)| {
                tracing::debug_span!("`upload_code` generator", call_id = i + 1)
                    .in_scope(|| UploadCodeArgs::generate::<Rng>(code_seed))
            })
            .collect();

        Batch::UploadCode(inner)
    }

    #[instrument(skip_all, fields(seed = seed, batch_type = "send_message"))]
    fn gen_send_message_batch(
        &mut self,
        existing_programs: NonEmptyVec<ProgramId>,
        seed: Seed,
        rt_settings: RuntimeSettings,
    ) -> Batch {
        let mut rng = Rng::seed_from_u64(seed);
        let inner = utils::iterator_with_args(self.batch_size, || {
            (existing_programs.clone(), rng.next_u64())
        })
        .enumerate()
        .map(|(i, (existing_programs, rng_seed))| {
            tracing::debug_span!("`send_message` generator", call_id = i + 1).in_scope(|| {
                SendMessageArgs::generate::<Rng>(existing_programs, rng_seed, rt_settings.gas_limit)
            })
        })
        .collect();

        Batch::SendMessage(inner)
    }

    #[instrument(skip_all, fields(seed = seed, batch_type = "create_program"))]
    fn gen_create_program_batch(
        &mut self,
        existing_codes: NonEmptyVec<CodeId>,
        seed: Seed,
        rt_settings: RuntimeSettings,
    ) -> Batch {
        let mut rng = Rng::seed_from_u64(seed);
        let inner =
            utils::iterator_with_args(self.batch_size, || (existing_codes.clone(), rng.next_u64()))
                .enumerate()
                .map(|(i, (existing_programs, rng_seed))| {
                    tracing::debug_span!("`create_program` generator", call_id = i + 1).in_scope(
                        || {
                            CreateProgramArgs::generate::<Rng>(
                                existing_programs,
                                rng_seed,
                                rt_settings.gas_limit,
                            )
                        },
                    )
                })
                .collect();

        Batch::CreateProgram(inner)
    }
}
