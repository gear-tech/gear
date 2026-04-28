use super::seed;
use crate::{
    args::SeedVariant,
    batch::{WorkloadPolicy, context::Context},
};
use anyhow::Result;
use ethexe_common::DEFAULT_BLOCK_GAS_LIMIT;
use gear_call_gen::{
    CallArgs, CallGenRng, CallGenRngCore, ClaimValueArgs, CreateProgramArgs,
    PeerAwareGenerationContext, Seed, SendMessageArgs, SendReplyArgs, UploadCodeArgs,
    UploadProgramArgs, generate_upload_code_args_peer_aware,
    generate_upload_program_args_peer_aware,
};
use gear_utils::NonEmpty;
use gear_wasm_gen::StandardGearWasmConfigsBundle;
use std::iter;
use tracing::instrument;

/// Runtime values that need to stay in sync with the target `ethexe` network.
#[derive(Clone, Copy)]
pub struct RuntimeSettings {
    gas_limit: u64,
}

impl RuntimeSettings {
    /// Loads the runtime settings used when generating call arguments.
    pub fn new() -> Result<Self> {
        let gas_limit = DEFAULT_BLOCK_GAS_LIMIT;

        Ok(Self { gas_limit })
    }
}

/// Stateful random batch generator used by the worker pool.
pub struct BatchGenerator<Rng> {
    pub batch_gen_rng: Rng,
    pub batch_size: usize,
    code_seed_gen: Box<dyn CallGenRngCore>,
    rt_settings: RuntimeSettings,
    workload_policy: WorkloadPolicy,
}

/// One logical group of homogeneous operations that a worker can execute.
#[derive(Debug)]
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

/// A generated batch together with the seed that produced it.
pub struct BatchWithSeed {
    pub seed: Seed,
    pub batch: Batch,
}

impl BatchWithSeed {
    /// Returns a stable human-readable name for logging and diagnostics.
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
    /// Creates a new batch generator for the provided loader seed.
    pub fn new(
        seed: Seed,
        batch_size: usize,
        code_seed_type: Option<SeedVariant>,
        rt_settings: RuntimeSettings,
        workload_policy: WorkloadPolicy,
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
            workload_policy,
        }
    }

    fn select_batch_id(&mut self, context: &Context) -> u8 {
        if context.active_program_ids().is_empty() {
            return self.select_program_creation_batch_id(context);
        }

        if self.batch_gen_rng.gen_range(0..100u8) < self.workload_policy.program_creation_ratio {
            self.select_program_creation_batch_id(context)
        } else {
            self.select_non_creation_batch_id(context)
        }
    }

    fn select_program_creation_batch_id(&mut self, context: &Context) -> u8 {
        if context.all_code_ids().is_empty() || self.batch_gen_rng.gen_bool(0.5) {
            0
        } else {
            3
        }
    }

    fn select_non_creation_batch_id(&mut self, context: &Context) -> u8 {
        let mut viable = vec![1, 2];
        if !context.all_mailbox_message_ids().is_empty() {
            viable.extend([4, 5]);
        }

        let idx = self.batch_gen_rng.gen_range(0..viable.len());
        viable[idx]
    }

    /// Produces the next batch using the current shared execution context.
    pub fn generate(&mut self, context: Context) -> BatchWithSeed {
        let seed = self.batch_gen_rng.next_u64();
        let batch_id = self.select_batch_id(&context);
        let rt_settings = self.rt_settings;

        let batch = self.generate_batch(batch_id, context, seed, rt_settings);

        (seed, batch).into()
    }

    /// Selects a batch family and fills it with generated call arguments.
    ///
    /// When the chosen batch type needs existing programs, codes, or mailbox
    /// entries and the context does not yet contain any, the generator falls
    /// back to an upload batch so the system can make forward progress.
    fn generate_batch(
        &mut self,
        batch_id: u8,
        context: Context,
        seed: Seed,
        rt_settings: RuntimeSettings,
    ) -> Batch {
        match batch_id {
            0 => self.generate_upload_program_batch(context, seed, rt_settings.gas_limit),
            1 => self.generate_upload_code_batch(context, seed),
            2 => match NonEmpty::from_vec(context.active_program_ids()) {
                Some(existing_programs) => Self::gen_batch::<SendMessageArgs, _, _>(
                    self.batch_size,
                    seed,
                    |rng| (existing_programs.clone(), rng.next_u64()),
                    || (rt_settings.gas_limit,),
                ),
                None => self.generate_batch(0, context, seed, rt_settings),
            },
            3 => match NonEmpty::from_vec(context.all_code_ids()) {
                Some(existing_codes) => Self::gen_batch::<CreateProgramArgs, _, _>(
                    self.batch_size,
                    seed,
                    |rng| (existing_codes.clone(), rng.next_u64()),
                    || (rt_settings.gas_limit,),
                ),
                None => self.generate_batch(0, context, seed, rt_settings),
            },
            4 => match NonEmpty::from_vec(context.all_mailbox_message_ids()) {
                Some(mailbox_messages) => Self::gen_batch::<SendReplyArgs, _, _>(
                    self.batch_size,
                    seed,
                    |rng| (mailbox_messages.clone(), rng.next_u64()),
                    || (rt_settings.gas_limit,),
                ),
                None => self.generate_batch(0, context, seed, rt_settings),
            },
            5 => match NonEmpty::from_vec(context.all_mailbox_message_ids()) {
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

    #[instrument(skip_all, fields(seed = seed, batch_type = "upload_program"))]
    fn generate_upload_program_batch(
        &mut self,
        context: Context,
        seed: Seed,
        gas_limit: u64,
    ) -> Batch {
        let peer_ctx = Self::peer_aware_generation_context(
            context,
            seed,
            self.workload_policy.persistent_program_loading(),
        );
        Self::log_peer_aware_generation_context("upload_program", &peer_ctx);
        let mut rng = Rng::seed_from_u64(seed);
        let batch = iter::repeat_with(|| {
            let code_seed = self.code_seed_gen.next_u64();
            let rng_seed = rng.next_u64();
            generate_upload_program_args_peer_aware::<Rng>(
                code_seed,
                rng_seed,
                gas_limit,
                peer_ctx.clone(),
            )
        })
        .take(self.batch_size)
        .collect();

        Batch::UploadProgram(batch)
    }

    #[instrument(skip_all, fields(seed = seed, batch_type = "upload_code"))]
    fn generate_upload_code_batch(&mut self, context: Context, seed: Seed) -> Batch {
        let peer_ctx = Self::peer_aware_generation_context(
            context,
            seed,
            self.workload_policy.persistent_program_loading(),
        );
        Self::log_peer_aware_generation_context("upload_code", &peer_ctx);
        let batch = iter::repeat_with(|| {
            let code_seed = self.code_seed_gen.next_u64();
            generate_upload_code_args_peer_aware::<Rng>(code_seed, peer_ctx.clone())
        })
        .take(self.batch_size)
        .collect();

        Batch::UploadCode(batch)
    }

    fn log_peer_aware_generation_context(
        batch_type: &'static str,
        peer_ctx: &PeerAwareGenerationContext,
    ) {
        if let Some(log_info) = peer_ctx.log_info.as_deref() {
            tracing::info!(batch_type, %log_info, "Peer-aware generation context");
        }
    }

    fn peer_aware_generation_context(
        context: Context,
        seed: Seed,
        suppress_exit: bool,
    ) -> PeerAwareGenerationContext {
        let known_programs = context.all_program_ids();
        let active_programs = context.active_program_ids();
        let active_program_count = active_programs.len();
        let known_codes = context.all_code_ids();
        let tracked_mailbox_owners = context
            .all_mailbox_message_ids()
            .into_iter()
            .filter_map(|message_id| context.owner_of(message_id))
            .count();

        PeerAwareGenerationContext {
            programs: NonEmpty::from_vec(active_programs),
            codes: NonEmpty::from_vec(known_codes),
            log_info: Some(format!(
                "Gear program seed = '{seed}', known programs = {}, active programs = {}, tracked mailbox owners = {}",
                known_programs.len(),
                active_program_count,
                tracked_mailbox_owners,
            )),
            suppress_exit,
        }
    }

    /// Generates a homogeneous batch of call arguments from a deterministic seed.
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

#[cfg(test)]
mod tests {
    use super::{Batch, BatchGenerator, RuntimeSettings};
    use crate::{
        args::SeedVariant,
        batch::{
            WorkloadPolicy,
            context::{Context, ContextUpdate},
        },
    };
    use gprimitives::{ActorId, CodeId, MessageId};
    use rand::rngs::SmallRng;

    fn actor(seed: u8) -> ActorId {
        ActorId::from([seed; 32])
    }

    fn code(seed: u8) -> CodeId {
        CodeId::from([seed; 32])
    }

    fn message(seed: u8) -> MessageId {
        MessageId::from([seed; 32])
    }

    fn context_with_programs() -> Context {
        let active = actor(1);
        let exited = actor(2);
        let known_code = code(3);
        let mailbox_mid = message(4);

        let mut update = ContextUpdate::default();
        update.add_code(known_code);
        update.set_program_code_id(active, known_code);
        update.add_mailbox_message(active, mailbox_mid);
        update.upsert_message_owner(mailbox_mid, active);
        update.set_program_code_id(exited, known_code);
        update.set_program_exited(exited, true);

        let mut context = Context::new();
        context.update(update);
        context
    }

    #[test]
    fn peer_context_uses_active_programs_only() {
        let peer_ctx = BatchGenerator::<SmallRng>::peer_aware_generation_context(
            context_with_programs(),
            77,
            false,
        );

        let programs = peer_ctx.programs.expect("active program list present");
        let collected: Vec<_> = programs.into_iter().collect();
        assert_eq!(collected, vec![actor(1)]);
    }

    #[test]
    fn upload_batches_use_peer_aware_helpers() {
        let mut generator = BatchGenerator::<SmallRng>::new(
            10,
            2,
            Some(SeedVariant::Constant(11)),
            RuntimeSettings { gas_limit: 123 },
            WorkloadPolicy::new(None),
        );

        match generator.generate_batch(
            0,
            context_with_programs(),
            12,
            RuntimeSettings { gas_limit: 123 },
        ) {
            Batch::UploadProgram(batch) => {
                assert_eq!(batch.len(), 2);
                assert!(batch.iter().all(|args| !args.0.0.is_empty()));
            }
            other => panic!("unexpected batch: {other:?}"),
        }

        match generator.generate_batch(
            1,
            context_with_programs(),
            13,
            RuntimeSettings { gas_limit: 123 },
        ) {
            Batch::UploadCode(batch) => {
                assert_eq!(batch.len(), 2);
                assert!(batch.iter().all(|args| !args.0.is_empty()));
            }
            other => panic!("unexpected batch: {other:?}"),
        }
    }

    #[test]
    fn mailbox_selection_comes_from_richer_context() {
        let mut generator = BatchGenerator::<SmallRng>::new(
            21,
            1,
            Some(SeedVariant::Constant(22)),
            RuntimeSettings { gas_limit: 321 },
            WorkloadPolicy::new(None),
        );

        match generator.generate_batch(
            4,
            context_with_programs(),
            23,
            RuntimeSettings { gas_limit: 321 },
        ) {
            Batch::SendReply(batch) => assert_eq!(batch[0].0.0, message(4)),
            other => panic!("unexpected batch: {other:?}"),
        }
    }
}
