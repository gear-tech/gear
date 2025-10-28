use crate::{ProcessorContext, ProcessorExternalities, ext::ExtInfo};
use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};
use gear_core::{
    buffer::Payload,
    code::Code,
    gas::{GasAllowanceCounter, GasCounter, ValueCounter},
    gas_metering::CustomConstantCostRules,
    ids::{ActorId, CodeId, prelude::*},
    memory::{AllocationsContext, PageBuf},
    message::{ContextSettings, DispatchKind, IncomingDispatch, IncomingMessage, MessageContext},
    pages::GearPage,
};
use gear_core_backend::{
    MemorySnapshot, MemorySnapshotStrategy, NoopSnapshot,
    env::{BackendReport, Environment, ExecutedEnvironment, ReadyToExecute},
};
use gear_lazy_pages::{LazyPagesStorage, LazyPagesVersion};
use gear_lazy_pages_common::LazyPagesInitContext;
use gear_lazy_pages_native_interface::LazyPagesNative;
use parity_scale_codec::{Decode, Encode};

type Ext = crate::Ext<LazyPagesNative>;

// Keep the memory footprint small enough that we can reason about and
// validate the captured lazy-page dumps in tests.
const MEMORY_SIZE: u16 = 100;
// Cap maximum memory to ensure predictable addressing when dumping pages.
const MAX_MEMORY: u16 = 65_000;

#[derive(Copy, Clone, Eq, PartialEq)]
enum StepExpectation {
    ShouldSucceed,
    ShouldFail,
}

impl StepExpectation {
    fn expects_success(self) -> bool {
        matches!(self, StepExpectation::ShouldSucceed)
    }
}

// A single step in the scenario with an expectation about success/failure.
struct ExecutionStep {
    label: &'static str,
    kind: DispatchKind,
    payload: Payload,
    expectation: StepExpectation,
}

impl ExecutionStep {
    fn success(label: &'static str, kind: DispatchKind, payload: Payload) -> Self {
        Self {
            label,
            kind,
            payload,
            expectation: StepExpectation::ShouldSucceed,
        }
    }

    fn failure(label: &'static str, kind: DispatchKind, payload: Payload) -> Self {
        Self {
            label,
            kind,
            payload,
            expectation: StepExpectation::ShouldFail,
        }
    }
}

#[derive(Debug)]
struct EmptyStorage;

impl LazyPagesStorage for EmptyStorage {
    fn page_exists(&self, _key: &[u8]) -> bool {
        false
    }

    fn load_page(&mut self, _key: &[u8], _buffer: &mut [u8]) -> Option<u32> {
        unreachable!();
    }
}

// Thin helper that reads raw bytes from the captured lazy-page map.
struct MemoryView<'a> {
    pages: &'a BTreeMap<GearPage, PageBuf>,
}

impl<'a> MemoryView<'a> {
    fn new(info: &'a ExtInfo) -> Self {
        Self {
            pages: &info.pages_data,
        }
    }

    fn read_into(&self, offset: u32, buffer: &mut [u8]) {
        if buffer.is_empty() {
            return;
        }

        let mut written = 0usize;
        let mut cursor = offset;

        while written < buffer.len() {
            let page = GearPage::from_offset(cursor);
            let page_start = page.offset();
            let intra = (cursor - page_start) as usize;
            let available = (GearPage::SIZE as usize)
                .saturating_sub(intra)
                .min(buffer.len() - written);

            if let Some(data) = self.pages.get(&page) {
                buffer[written..written + available]
                    .copy_from_slice(&data[intra..intra + available]);
            } else {
                buffer[written..written + available].fill(0);
            }

            cursor = cursor
                .checked_add(available as u32)
                .expect("memory read overflow");
            written += available;
        }
    }

    fn read_array<const N: usize>(&self, offset: u32) -> [u8; N] {
        let mut buf = [0u8; N];
        self.read_into(offset, &mut buf);
        buf
    }

    fn read_u32(&self, offset: u32) -> u32 {
        u32::from_le_bytes(self.read_array(offset))
    }

    fn read_u128(&self, offset: u32) -> u128 {
        u128::from_le_bytes(self.read_array(offset))
    }

    fn read_bytes(&self, offset: u32, len: usize) -> Vec<u8> {
        let mut buf = vec![0u8; len];
        self.read_into(offset, &mut buf);
        buf
    }

    fn flatten(&self) -> Option<(u32, Vec<u8>)> {
        let (start_page, end_page) = self
            .pages
            .keys()
            .next()
            .zip(self.pages.keys().next_back())?;

        let start = start_page.offset();
        let end = end_page
            .offset()
            .checked_add(GearPage::SIZE)
            .expect("memory end overflow");
        let len = (end - start) as usize;

        Some((start, self.read_bytes(start, len)))
    }

    fn find_sequence(&self, needle: &[u8]) -> Option<u32> {
        if needle.is_empty() {
            return Some(0);
        }

        let (base, haystack) = self.flatten()?;
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
            .map(|idx| base + idx as u32)
    }

    fn find_all(&self, needle: &[u8]) -> Vec<u32> {
        if needle.is_empty() {
            return vec![];
        }

        self.flatten()
            .map(|(base, haystack)| {
                haystack
                    .windows(needle.len())
                    .enumerate()
                    .filter_map(|(idx, window)| (window == needle).then_some(base + idx as u32))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn read_utf8(&self, offset: u32, len: usize) -> String {
        let bytes = self.read_bytes(offset, len);
        String::from_utf8(bytes).expect("memory contains invalid UTF-8")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
// High-level view of the fungible token contract state reconstructed from memory.
struct TokenSnapshot {
    total_supply: u128,
    name: String,
    symbol: String,
    balances: Vec<(ActorId, u128)>,
}

impl TokenSnapshot {
    fn from_ext(info: &ExtInfo) -> Self {
        // Decode the contract state directly from the lazy-page dump.
        let memory = MemoryView::new(info);

        let name_ptr = memory
            .find_sequence(b"MyToken")
            .expect("contract name is missing in memory");
        let symbol_ptr = memory
            .find_sequence(b"MTK")
            .expect("contract symbol is missing in memory");

        let pointer_positions = memory.find_all(&name_ptr.to_le_bytes());
        let pointer_field = pointer_positions
            .into_iter()
            .filter(|pos| *pos < name_ptr)
            .min()
            .expect("name pointer field not found");
        let struct_base = pointer_field
            .checked_sub(52)
            .expect("failed to derive struct base");

        let total_supply = memory.read_u128(struct_base);

        let name_len = memory.read_u32(struct_base + 56) as usize;
        let name = memory.read_utf8(name_ptr, name_len);

        let symbol_len = memory.read_u32(struct_base + 68) as usize;
        let symbol = memory.read_utf8(symbol_ptr, symbol_len);

        let actor = message_sender();
        let pattern = actor.as_ref();
        let mut balances = memory
            .find_all(pattern)
            .into_iter()
            .map(|pos| {
                let amount = memory.read_u128(pos + pattern.len() as u32);
                (actor, amount)
            })
            .collect::<Vec<_>>();
        // Some tests run the same account through multiple paths; normalise by sorting/deduping.
        balances.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        balances.dedup();

        Self {
            total_supply,
            name,
            symbol,
            balances,
        }
    }
}

#[test]
fn execute_environment_multiple_times_with_memory_replacing() {
    use demo_fungible_token::{FTAction, InitConfig};

    let steps = vec![
        step_success("Init", DispatchKind::Init, InitConfig::test_sequence()),
        step_success("Mint", DispatchKind::Handle, FTAction::Mint(1_000_000)),
        step_failure(
            "Burn too much",
            DispatchKind::Handle,
            FTAction::Burn(2_000_000),
        ),
        step_success(
            "Burn allowed amount",
            DispatchKind::Handle,
            FTAction::Burn(300_000),
        ),
        step_success("TotalSupply", DispatchKind::Handle, FTAction::TotalSupply),
    ];

    let mut snapshot = Ext::memory_snapshot();
    let info = run_sequence(&steps, &mut snapshot);

    // After replaying a multi-step scenario we inspect the raw memory dump
    // and verify the high-level token state matches the business expectations.
    let token_snapshot = TokenSnapshot::from_ext(&info);
    let expected_actor = message_sender();
    assert_eq!(token_snapshot.total_supply, 700_000);
    assert_eq!(token_snapshot.name, "MyToken");
    assert_eq!(token_snapshot.symbol, "MTK");
    assert_eq!(token_snapshot.balances, vec![(expected_actor, 700_000)]);

    assert_pages_populated(&info);
    assert_total_supply_reply(&info);

    let mut snapshot_second = Ext::memory_snapshot();
    let info_second = run_sequence(&steps, &mut snapshot_second);

    let second_snapshot = TokenSnapshot::from_ext(&info_second);
    assert_eq!(token_snapshot, second_snapshot);

    assert_pages_equal(&info, &info_second);
}

#[test]
fn execute_environment_multiple_times_and_compare_results() {
    use demo_fungible_token::{FTAction, InitConfig};

    let steps_with_failure = vec![
        step_success("Init", DispatchKind::Init, InitConfig::test_sequence()),
        step_success("Mint", DispatchKind::Handle, FTAction::Mint(1_000_000)),
        step_failure(
            "Burn too much",
            DispatchKind::Handle,
            FTAction::Burn(2_000_000),
        ),
        step_success(
            "Burn allowed amount",
            DispatchKind::Handle,
            FTAction::Burn(300_000),
        ),
        step_success("TotalSupply", DispatchKind::Handle, FTAction::TotalSupply),
    ];

    let normal_sequence = vec![
        step_success("Init", DispatchKind::Init, InitConfig::test_sequence()),
        step_success("Mint", DispatchKind::Handle, FTAction::Mint(1_000_000)),
        step_success(
            "Burn allowed amount",
            DispatchKind::Handle,
            FTAction::Burn(300_000),
        ),
        step_success("TotalSupply", DispatchKind::Handle, FTAction::TotalSupply),
    ];

    let altered_sequence = vec![
        step_success("Init", DispatchKind::Init, InitConfig::test_sequence()),
        step_success("Mint", DispatchKind::Handle, FTAction::Mint(1_000_000)),
        step_success(
            "Burn different amount",
            DispatchKind::Handle,
            FTAction::Burn(500_000),
        ),
        step_success("TotalSupply", DispatchKind::Handle, FTAction::TotalSupply),
    ];

    let mut snapshot = Ext::memory_snapshot();

    let info_with_failure = run_sequence(&steps_with_failure, &mut snapshot);
    let info_normal = run_sequence(&normal_sequence, &mut snapshot);
    let info_altered = run_sequence(&altered_sequence, &mut snapshot);

    let actor = message_sender();
    let snapshot_with_failure = TokenSnapshot::from_ext(&info_with_failure);
    let snapshot_normal = TokenSnapshot::from_ext(&info_normal);
    let snapshot_altered = TokenSnapshot::from_ext(&info_altered);

    assert_eq!(snapshot_with_failure, snapshot_normal);
    assert_eq!(snapshot_with_failure.total_supply, 700_000);
    assert_eq!(snapshot_altered.total_supply, 500_000);
    assert_eq!(snapshot_altered.balances, vec![(actor, 500_000)]);

    // Failures revert the state, so both sequences should end up with identical memory dumps.
    assert_pages_equal(&info_with_failure, &info_normal);
    // Altering one of the commands yields a different memory snapshot.
    assert_pages_different(&info_normal, &info_altered);

    assert_total_supply_reply(&info_with_failure);
    assert_total_supply_reply(&info_normal);
    assert_total_supply_reply(&info_altered);
}

#[test]
fn execute_sequence_without_snapshots_diverges() {
    use demo_fungible_token::{FTAction, InitConfig};

    let steps = vec![
        step_success("Init", DispatchKind::Init, InitConfig::test_sequence()),
        step_success("Mint", DispatchKind::Handle, FTAction::Mint(1_000_000)),
        step_failure(
            "Burn (should fail)",
            DispatchKind::Handle,
            FTAction::Burn(2_000_000),
        ),
        step_success("TotalSupply", DispatchKind::Handle, FTAction::TotalSupply),
    ];

    let mut snapshot = Ext::memory_snapshot();
    let _ = run_sequence(&steps, &mut snapshot);

    let mut disabled = Ext::memory_snapshot();
    let result = run_sequence_without_snapshots(&steps, &mut disabled);

    assert!(result.is_none(), "Execution without snapshots should fail");
}

#[test]
fn execute_sequence_with_consecutive_failures() {
    use demo_fungible_token::{FTAction, InitConfig};

    let steps = vec![
        step_success("Init", DispatchKind::Init, InitConfig::test_sequence()),
        step_success("Mint", DispatchKind::Handle, FTAction::Mint(1_000_000)),
        step_failure("Burn 1", DispatchKind::Handle, FTAction::Burn(2_000_000)),
        step_failure("Burn 2", DispatchKind::Handle, FTAction::Burn(3_000_000)),
        step_success("Mint again", DispatchKind::Handle, FTAction::Mint(500_000)),
        step_success("TotalSupply", DispatchKind::Handle, FTAction::TotalSupply),
    ];

    let mut snapshot = Ext::memory_snapshot();
    let info = run_sequence(&steps, &mut snapshot);

    // Consecutive failures should not leak state across retries; once the run succeeds we confirm the final balances.
    let token_snapshot = TokenSnapshot::from_ext(&info);
    let actor = message_sender();
    assert_eq!(token_snapshot.total_supply, 1_500_000);
    assert_eq!(token_snapshot.balances, vec![(actor, 1_500_000)]);

    assert_pages_populated(&info);
    assert_total_supply_reply(&info);
}

// Execute the steps sequentially, applying memory snapshots on failures so we mimic
// the production lazy-pages recovery behaviour.
fn run_sequence<M: MemorySnapshot>(steps: &[ExecutionStep], snapshot: &mut M) -> ExtInfo {
    assert!(!steps.is_empty(), "sequence must contain at least one step");

    let fixture = ProgramFixture::load();
    let mut execution_result: Option<ExecutedEnvironment<Ext>> = None;
    let mut previous_expectation = StepExpectation::ShouldSucceed;
    let mut previous_label = "<init>";

    for (index, step) in steps.iter().enumerate() {
        let env = if index == 0 {
            create_environment(step, &fixture)
        } else {
            advance_environment(
                execution_result.take().expect("missing execution result"),
                step,
                previous_expectation,
                snapshot,
                previous_label,
                &fixture,
            )
        };

        let result = env
            .execute(step.kind, MemorySnapshotStrategy::enabled(snapshot))
            .unwrap_or_else(|err| panic!("Failed to execute step `{}`: {err}", step.label));

        previous_expectation = step.expectation;
        previous_label = step.label;
        execution_result = Some(result);
    }

    finalize_execution(
        execution_result.expect("execution sequence must produce result"),
        previous_label,
    )
}

fn run_sequence_without_snapshots<M: MemorySnapshot>(
    steps: &[ExecutionStep],
    _snapshot: &mut M,
) -> Option<ExtInfo> {
    assert!(!steps.is_empty(), "sequence must contain at least one step");

    let fixture = ProgramFixture::load();
    let mut execution_result: Option<ExecutedEnvironment<Ext>> = None;

    for (index, step) in steps.iter().enumerate() {
        let env = if let Some(result) = execution_result.take() {
            match result {
                ExecutedEnvironment::SuccessExecution(success) => success,
                ExecutedEnvironment::FailedExecution(_) => return None,
            }
            .set_ext(make_ext(step, fixture.program_id, &fixture.code))
            .unwrap()
        } else {
            if index != 0 {
                return None;
            }
            create_environment(step, &fixture)
        };

        let execution = match env
            .execute(
                step.kind,
                MemorySnapshotStrategy::<NoopSnapshot>::disabled(),
            )
            .ok()?
        {
            ExecutedEnvironment::SuccessExecution(exec) => exec,
            ExecutedEnvironment::FailedExecution(_) => return None,
        };

        execution_result = Some(ExecutedEnvironment::SuccessExecution(execution));
    }

    execution_result.map(|res| finalize_execution(res, steps.last().unwrap().label))
}

fn create_environment<'a>(
    step: &ExecutionStep,
    fixture: &'a ProgramFixture,
) -> Environment<'a, Ext, ReadyToExecute> {
    let ext = make_ext(step, fixture.program_id, &fixture.code);
    Environment::new(
        ext,
        fixture.code.instrumented_code().bytes(),
        fixture.code.metadata().exports().clone(),
        MEMORY_SIZE.into(),
        |ctx, mem, globals_config| {
            Ext::lazy_pages_init_for_program(
                ctx,
                mem,
                fixture.program_id,
                Default::default(),
                fixture.code.metadata().stack_end(),
                globals_config,
                Default::default(),
            );
        },
    )
    .unwrap()
}

fn advance_environment<'a, M: MemorySnapshot>(
    execution_result: ExecutedEnvironment<'a, Ext>,
    next_step: &ExecutionStep,
    previous_expectation: StepExpectation,
    snapshot: &mut M,
    previous_label: &str,
    fixture: &'a ProgramFixture,
) -> Environment<'a, Ext, ReadyToExecute> {
    match execution_result {
        ExecutedEnvironment::SuccessExecution(success) => {
            if !previous_expectation.expects_success() {
                panic!("Step `{previous_label}` was expected to fail, but finished successfully");
            }

            success
                .set_ext(make_ext(next_step, fixture.program_id, &fixture.code))
                .unwrap()
        }
        ExecutedEnvironment::FailedExecution(failed) => {
            if previous_expectation.expects_success() {
                panic!("Step `{previous_label}` was expected to succeed, but failed instead");
            }

            let reverted = failed
                .revert(snapshot)
                .unwrap_or_else(|_| panic!("Failed to revert memory at `{previous_label}`"));

            reverted
                .set_ext(make_ext(next_step, fixture.program_id, &fixture.code))
                .unwrap()
        }
    }
}

fn finalize_execution(execution_result: ExecutedEnvironment<'_, Ext>, last_label: &str) -> ExtInfo {
    let success_execution = match execution_result {
        ExecutedEnvironment::SuccessExecution(exec) => exec,
        ExecutedEnvironment::FailedExecution(_) => {
            panic!("Execution result is failed, last successful step: `{last_label}`")
        }
    };

    let BackendReport {
        mut store,
        mut memory,
        ext,
        ..
    } = success_execution
        .report()
        .expect("Failed to finalize environment report");

    Ext::lazy_pages_post_execution_actions(&mut store, &mut memory);

    ext.into_ext_info(&mut store, &memory)
        .expect("Failed to extract externalities info")
}

struct ProgramFixture {
    code: Code,
    program_id: ActorId,
}

impl ProgramFixture {
    fn load() -> Self {
        use demo_fungible_token::WASM_BINARY;

        const PROGRAM_STORAGE_PREFIX: [u8; 32] = *b"execute_wasm_multiple_times_test";

        gear_lazy_pages::init(
            LazyPagesVersion::Version1,
            LazyPagesInitContext::new(PROGRAM_STORAGE_PREFIX),
            EmptyStorage,
        )
        .expect("Failed to init lazy-pages");

        let code_bytes = WASM_BINARY.to_vec();

        let code = Code::try_new(
            code_bytes,
            1,
            |_| CustomConstantCostRules::new(0, 0, 0),
            None,
            None,
            None,
            None,
        )
        .expect("Failed to create Code");

        let code_id = CodeId::generate(code.original_code());
        let program_id = ActorId::generate_from_user(code_id, b"");

        Self { code, program_id }
    }
}

fn step_success<T: Encode>(label: &'static str, kind: DispatchKind, value: T) -> ExecutionStep {
    ExecutionStep::success(label, kind, encoded_payload(value))
}

fn step_failure<T: Encode>(label: &'static str, kind: DispatchKind, value: T) -> ExecutionStep {
    ExecutionStep::failure(label, kind, encoded_payload(value))
}

fn encoded_payload<T: Encode>(value: T) -> Payload {
    Payload::try_from(value.encode()).expect("Failed to encode payload")
}

fn assert_pages_populated(info: &ExtInfo) {
    assert!(
        !info.pages_data.is_empty(),
        "Expected lazy page snapshot to contain modified pages"
    );

    let has_non_zero = info
        .pages_data
        .values()
        .any(|buf| buf.iter().any(|byte| *byte != 0));
    assert!(has_non_zero, "Captured pages should contain non-zero data");
}

fn assert_pages_equal(expected: &ExtInfo, actual: &ExtInfo) {
    assert_eq!(expected.pages_data, actual.pages_data);
}

fn assert_pages_different(expected: &ExtInfo, actual: &ExtInfo) {
    assert_ne!(expected.pages_data, actual.pages_data);
}

fn assert_total_supply_reply(info: &ExtInfo) {
    assert!(
        total_supply(info).is_some(),
        "Expected TotalSupply reply in the execution trace"
    );
}

fn total_supply(info: &ExtInfo) -> Option<u128> {
    info.generated_dispatches
        .iter()
        .find_map(|(dispatch, _, _)| {
            if dispatch.kind() != DispatchKind::Reply {
                return None;
            }

            let mut bytes = dispatch.payload_bytes();
            Decode::decode(&mut bytes).ok()
        })
}

fn make_ext(step: &ExecutionStep, actor_id: ActorId, code: &Code) -> Ext {
    let incoming_message = IncomingMessage::new(
        0.into(),
        message_sender(),
        step.payload.clone(),
        Default::default(),
        Default::default(),
        Default::default(),
    );

    let message_context = MessageContext::new(
        IncomingDispatch::new(step.kind, incoming_message, None),
        actor_id,
        ContextSettings::with_outgoing_limits(1024, u32::MAX),
    );

    let processor_context = ProcessorContext {
        message_context,
        program_id: actor_id,
        value_counter: ValueCounter::new(250_000_000_000),
        gas_counter: GasCounter::new(250_000_000_000),
        gas_allowance_counter: GasAllowanceCounter::new(250_000_000_000),
        allocations_context: AllocationsContext::try_new(
            MEMORY_SIZE.into(),
            Default::default(),
            code.metadata().static_pages(),
            code.metadata().stack_end(),
            MAX_MEMORY.into(),
        )
        .expect("Failed to create AllocationsContext"),
        ..ProcessorContext::new_mock()
    };

    Ext::new(processor_context)
}

fn message_sender() -> ActorId {
    let bytes = [1, 2, 3, 4].repeat(8);
    ActorId::try_from(bytes.as_ref()).unwrap()
}
