use super::seed;
use crate::{
    args::SeedVariant,
    batch_pool::{api::GearApiFacade, context::Context, Seed},
    utils,
};
use anyhow::Result;
use futures::FutureExt;
use gear_call_gen::{
    CallGenRng, CallGenRngCore, ClaimValueArgs, CreateProgramArgs, GearProgGenConfig,
    SendMessageArgs, SendReplyArgs, UploadCodeArgs, UploadProgramArgs,
};
use gear_core::ids::{CodeId, MessageId, ProgramId};
use gear_utils::NonEmpty;
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
                self.gen_upload_program_batch(existing_programs, seed, rt_settings)
            }
            1 => {
                let existing_programs = context.programs.iter().copied().collect::<Vec<_>>();
                let span = tracing::debug_span!(
                    "gen_upload_code_batch",
                    seed = seed,
                    batch_type = "upload_code"
                );
                span.in_scope(|| self.gen_upload_code_batch(existing_programs))
            }
            2 => match NonEmpty::from_vec(context.programs.iter().copied().collect()) {
                Some(existing_programs) => {
                    self.gen_send_message_batch(existing_programs, seed, rt_settings)
                }
                None => self.generate_batch(0, context, seed, rt_settings),
            },
            3 => match NonEmpty::from_vec(context.codes.iter().copied().collect()) {
                Some(existing_codes) => {
                    self.gen_create_program_batch(existing_codes, seed, rt_settings)
                }
                None => self.generate_batch(0, context, seed, rt_settings),
            },
            4 => match NonEmpty::from_vec(context.mailbox_state.iter().copied().collect()) {
                Some(mailbox_messages) => {
                    self.gen_send_reply_batch(mailbox_messages, seed, rt_settings)
                }
                None => self.generate_batch(0, context, seed, rt_settings),
            },
            5 => match NonEmpty::from_vec(context.mailbox_state.iter().copied().collect()) {
                Some(mailbox_messages) => self.gen_claim_value_batch(mailbox_messages, seed),
                None => self.generate_batch(0, context, seed, rt_settings),
            },
            _ => unreachable!(),
        }
    }

    #[instrument(skip_all, fields(seed = seed, batch_type = "upload_program"))]
    fn gen_upload_program_batch(
        &mut self,
        existing_programs: Vec<ProgramId>,
        seed: Seed,
        rt_settings: RuntimeSettings,
    ) -> Batch {
        let mut rng = Rng::seed_from_u64(seed);
        let inner = utils::iterator_with_args(self.batch_size, || {
            (
                existing_programs.clone(),
                self.code_seed_gen.next_u64(),
                rng.next_u64(),
            )
        })
        .enumerate()
        .map(|(i, (existing_programs, code_seed, rng_seed))| {
            tracing::debug_span!("`upload_program` generator", call_id = i + 1).in_scope(|| {
                UploadProgramArgs::generate::<Rng>(
                    code_seed,
                    rng_seed,
                    rt_settings.gas_limit,
                    self.prog_gen_config.clone(),
                    existing_programs,
                )
            })
        })
        .collect();

        Batch::UploadProgram(inner)
    }

    fn gen_upload_code_batch(&mut self, existing_programs: Vec<ProgramId>) -> Batch {
        let inner = utils::iterator_with_args(self.batch_size, || {
            (existing_programs.clone(), self.code_seed_gen.next_u64())
        })
        .enumerate()
        .map(|(i, (existing_programs, code_seed))| {
            tracing::debug_span!("`upload_code` generator", call_id = i + 1).in_scope(|| {
                UploadCodeArgs::generate::<Rng>(
                    code_seed,
                    self.prog_gen_config.clone(),
                    existing_programs,
                )
            })
        })
        .collect();

        Batch::UploadCode(inner)
    }

    #[instrument(skip_all, fields(seed = seed, batch_type = "send_message"))]
    fn gen_send_message_batch(
        &mut self,
        existing_programs: NonEmpty<ProgramId>,
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
        existing_codes: NonEmpty<CodeId>,
        seed: Seed,
        rt_settings: RuntimeSettings,
    ) -> Batch {
        let mut rng = Rng::seed_from_u64(seed);
        let inner =
            utils::iterator_with_args(self.batch_size, || (existing_codes.clone(), rng.next_u64()))
                .enumerate()
                .map(|(i, (existing_codes, rng_seed))| {
                    tracing::debug_span!("`create_program` generator", call_id = i + 1).in_scope(
                        || {
                            CreateProgramArgs::generate::<Rng>(
                                existing_codes,
                                rng_seed,
                                rt_settings.gas_limit,
                            )
                        },
                    )
                })
                .collect();

        Batch::CreateProgram(inner)
    }

    #[instrument(skip_all, fields(seed = seed, batch_type = "send_reply"))]
    fn gen_send_reply_batch(
        &mut self,
        mailbox_messages: NonEmpty<MessageId>,
        seed: Seed,
        rt_settings: RuntimeSettings,
    ) -> Batch {
        let mut rng = Rng::seed_from_u64(seed);
        let inner = utils::iterator_with_args(self.batch_size, || {
            (mailbox_messages.clone(), rng.next_u64())
        })
        .enumerate()
        .map(|(i, (mailbox_messages, rng_seed))| {
            tracing::debug_span!("`send_reply` generator", call_id = i + 1).in_scope(|| {
                SendReplyArgs::generate::<Rng>(mailbox_messages, rng_seed, rt_settings.gas_limit)
            })
        })
        .collect();

        Batch::SendReply(inner)
    }

    #[instrument(skip_all, fields(seed = seed, batch_type = "claim_value"))]
    fn gen_claim_value_batch(
        &mut self,
        mailbox_messages: NonEmpty<MessageId>,
        seed: Seed,
    ) -> Batch {
        let mut rng = Rng::seed_from_u64(seed);
        let inner = utils::iterator_with_args(self.batch_size, || {
            (mailbox_messages.clone(), rng.next_u64())
        })
        .enumerate()
        .map(|(i, (mailbox_messages, rng_seed))| {
            tracing::debug_span!("`claim_value` generator", call_id = i + 1)
                .in_scope(|| ClaimValueArgs::generate::<Rng>(mailbox_messages, rng_seed))
        })
        .collect();

        Batch::ClaimValue(inner)
    }
}
