use crate::{
    CallGenRng, Seed, UploadCodeArgs, UploadProgramArgs, generate_gear_program_with_builder,
};
use gear_core::ids::{ActorId, CodeId};
use gear_utils::NonEmpty;
use gear_wasm_gen::{
    ActorKind, PtrParamAllowedValues, RandomizedGearWasmConfigBundle, RegularParamType,
    SyscallsParamsConfig,
};
use std::ops::RangeInclusive;

const ZERO_VALUE_RANGE: RangeInclusive<u128> = 0..=0;

#[derive(Debug, Clone, Default)]
pub struct PeerAwareGenerationContext {
    pub programs: Option<NonEmpty<ActorId>>,
    pub codes: Option<NonEmpty<CodeId>>,
    pub log_info: Option<String>,
}

pub fn generate_upload_program_args_peer_aware<Rng: CallGenRng>(
    code_seed: Seed,
    rng_seed: Seed,
    gas_limit: u64,
    ctx: PeerAwareGenerationContext,
) -> UploadProgramArgs {
    let mut rng = Rng::seed_from_u64(rng_seed);
    let code = generate_peer_aware_program::<Rng>(code_seed, ctx.clone());

    let mut salt = vec![0; rng.gen_range(1..=100)];
    rng.fill_bytes(&mut salt);

    let mut payload = vec![0; rng.gen_range(1..=100)];
    rng.fill_bytes(&mut payload);

    UploadProgramArgs((code, salt, payload, gas_limit, 0))
}

pub fn generate_upload_code_args_peer_aware<Rng: CallGenRng>(
    code_seed: Seed,
    ctx: PeerAwareGenerationContext,
) -> UploadCodeArgs {
    UploadCodeArgs(generate_peer_aware_program::<Rng>(code_seed, ctx))
}

fn generate_peer_aware_program<Rng: CallGenRng>(
    code_seed: Seed,
    ctx: PeerAwareGenerationContext,
) -> Vec<u8> {
    generate_gear_program_with_builder::<Rng, _, _>(code_seed, |u| peer_aware_config(u, ctx))
}

fn peer_aware_config(
    unstructured: &mut arbitrary::Unstructured<'_>,
    ctx: PeerAwareGenerationContext,
) -> RandomizedGearWasmConfigBundle {
    let initial_pages = 2;
    let actor_kind = ctx
        .programs
        .and_then(|non_empty| NonEmpty::collect(non_empty.into_iter().map(|pid| pid.into())))
        .map(ActorKind::ExistingAddresses)
        .unwrap_or(ActorKind::Source);

    let mut params_config = SyscallsParamsConfig::new()
        .with_default_regular_config()
        .with_rule(RegularParamType::Alloc, (10..=20).into())
        .with_rule(
            RegularParamType::Free,
            (initial_pages..=initial_pages + 35).into(),
        )
        .with_ptr_rule(PtrParamAllowedValues::Value(ZERO_VALUE_RANGE.clone()))
        .with_ptr_rule(PtrParamAllowedValues::ActorId(actor_kind.clone()))
        .with_ptr_rule(PtrParamAllowedValues::ActorIdWithValue {
            actor_kind: actor_kind.clone(),
            range: ZERO_VALUE_RANGE.clone(),
        })
        .with_ptr_rule(PtrParamAllowedValues::ReservationIdWithValue(
            ZERO_VALUE_RANGE.clone(),
        ))
        .with_ptr_rule(PtrParamAllowedValues::ReservationIdWithActorIdAndValue {
            actor_kind,
            range: ZERO_VALUE_RANGE.clone(),
        })
        .with_ptr_rule(PtrParamAllowedValues::ReservationId)
        .with_ptr_rule(PtrParamAllowedValues::WaitedMessageId);

    if let Some(code_ids) = ctx.codes {
        params_config = params_config.with_ptr_rule(PtrParamAllowedValues::CodeIdsWithValue {
            code_ids,
            range: ZERO_VALUE_RANGE,
        });
    }

    RandomizedGearWasmConfigBundle::new_arbitrary(unstructured, ctx.log_info, params_config)
}

#[cfg(test)]
mod tests {
    use super::{
        PeerAwareGenerationContext, generate_upload_code_args_peer_aware,
        generate_upload_program_args_peer_aware,
    };
    use crate::generate_gear_program;
    use gear_core::ids::{ActorId, CodeId};
    use gear_utils::NonEmpty;
    use gear_wasm_gen::StandardGearWasmConfigsBundle;
    use rand::rngs::SmallRng;

    fn actor(seed: u8) -> ActorId {
        ActorId::from([seed; 32])
    }

    fn code(seed: u8) -> CodeId {
        CodeId::from([seed; 32])
    }

    #[test]
    fn peer_aware_generation_falls_back_without_known_peers() {
        let ctx = PeerAwareGenerationContext {
            log_info: Some("no-peers".into()),
            ..Default::default()
        };

        let program = generate_upload_program_args_peer_aware::<SmallRng>(1, 2, 123, ctx.clone());
        let code = generate_upload_code_args_peer_aware::<SmallRng>(1, ctx);

        assert!(!program.0.0.is_empty());
        assert!(!code.0.is_empty());
    }

    #[test]
    fn peer_aware_generation_is_deterministic_for_fixed_peers() {
        let ctx = PeerAwareGenerationContext {
            programs: Some(NonEmpty::new(actor(1))),
            codes: Some(NonEmpty::new(code(2))),
            log_info: Some("fixed-peers".into()),
        };

        let first = generate_upload_program_args_peer_aware::<SmallRng>(7, 9, 10, ctx.clone());
        let second = generate_upload_program_args_peer_aware::<SmallRng>(7, 9, 10, ctx);

        assert_eq!(first, second);
    }

    #[test]
    fn legacy_generate_gear_program_stays_deterministic() {
        let first = generate_gear_program::<SmallRng, StandardGearWasmConfigsBundle>(
            42,
            StandardGearWasmConfigsBundle::default(),
        );
        let second = generate_gear_program::<SmallRng, StandardGearWasmConfigsBundle>(
            42,
            StandardGearWasmConfigsBundle::default(),
        );

        assert_eq!(first, second);
    }

    #[test]
    fn peer_aware_generation_accepts_known_programs_and_codes() {
        let ctx = PeerAwareGenerationContext {
            programs: Some(NonEmpty::new(actor(9))),
            codes: Some(NonEmpty::new(code(10))),
            log_info: Some("with-peers".into()),
        };

        let program = generate_upload_program_args_peer_aware::<SmallRng>(11, 12, 13, ctx.clone());
        let code = generate_upload_code_args_peer_aware::<SmallRng>(11, ctx);

        assert!(!program.0.0.is_empty());
        assert!(!code.0.is_empty());
    }
}
