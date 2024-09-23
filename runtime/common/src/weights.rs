use gear_core::{
    costs::{CostOf, LazyPagesCosts},
    pages::GearPagesAmount,
};
use pallet_gear::{InstructionWeights, MemoryWeights, SyscallWeights};

const INSTRUCTIONS_SPREAD: u8 = 50;
const SYSCALL_SPREAD: u8 = 10;
const PAGES_SPREAD: u8 = 10;

#[derive(Clone, Copy)]
struct WeightExpectation {
    weight: u64,
    expected: u64,
    spread: u8,
    name: &'static str,
}

impl WeightExpectation {
    fn new(weight: u64, expected: u64, spread: u8, name: &'static str) -> Self {
        Self {
            weight,
            expected,
            spread,
            name,
        }
    }

    fn check(&self) -> Result<(), String> {
        let left = self.expected - self.expected * self.spread as u64 / 100;
        let right = self.expected + self.expected * self.spread as u64 / 100;

        if left > self.weight || self.weight > right {
            return Err(format!("Instruction [{}]. Weight is {} ps. Expected weight is {} ps. {}% spread interval: [{left} ps, {right} ps]", self.name, self.weight, self.expected, self.spread));
        }

        Ok(())
    }
}

fn check_expectations(expectations: &[WeightExpectation]) -> Result<(), Vec<String>> {
    let errors = expectations
        .iter()
        .filter_map(|expectation| {
            if let Err(err) = expectation.check() {
                Some(err)
            } else {
                None
            }
        })
        .collect::<Vec<String>>();

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Check that the weights of instructions are within the expected range
pub fn check_instructions_weights<T: pallet_gear::Config>(
    weights: InstructionWeights<T>,
    expected: InstructionWeights<T>,
) -> Result<(), Vec<String>> {
    macro_rules! expectation {
        ($inst_name:ident) => {
            WeightExpectation::new(
                weights.$inst_name.into(),
                expected.$inst_name.into(),
                INSTRUCTIONS_SPREAD,
                stringify!($inst_name),
            )
        };
    }

    let expectations = vec![
        expectation!(i64const),
        expectation!(i64load),
        expectation!(i32load),
        expectation!(i64store),
        expectation!(i32store),
        expectation!(select),
        expectation!(r#if),
        expectation!(br),
        expectation!(br_if),
        expectation!(br_table),
        expectation!(br_table_per_entry),
        expectation!(call),
        expectation!(call_indirect),
        expectation!(call_indirect_per_param),
        expectation!(call_per_local),
        expectation!(local_get),
        expectation!(local_set),
        expectation!(local_tee),
        expectation!(global_get),
        expectation!(global_set),
        expectation!(memory_current),
        expectation!(i64clz),
        expectation!(i32clz),
        expectation!(i64ctz),
        expectation!(i32ctz),
        expectation!(i64popcnt),
        expectation!(i32popcnt),
        expectation!(i64eqz),
        expectation!(i32eqz),
        expectation!(i32extend8s),
        expectation!(i32extend16s),
        expectation!(i64extend8s),
        expectation!(i64extend16s),
        expectation!(i64extend32s),
        expectation!(i64extendsi32),
        expectation!(i64extendui32),
        expectation!(i32wrapi64),
        expectation!(i64eq),
        expectation!(i32eq),
        expectation!(i64ne),
        expectation!(i32ne),
        expectation!(i64lts),
        expectation!(i32lts),
        expectation!(i64ltu),
        expectation!(i32ltu),
        expectation!(i64gts),
        expectation!(i32gts),
        expectation!(i64gtu),
        expectation!(i32gtu),
        expectation!(i64les),
        expectation!(i32les),
        expectation!(i64leu),
        expectation!(i32leu),
        expectation!(i64ges),
        expectation!(i32ges),
        expectation!(i64geu),
        expectation!(i32geu),
        expectation!(i64add),
        expectation!(i32add),
        expectation!(i64sub),
        expectation!(i32sub),
        expectation!(i64mul),
        expectation!(i32mul),
        expectation!(i64divs),
        expectation!(i32divs),
        expectation!(i64divu),
        expectation!(i32divu),
        expectation!(i64rems),
        expectation!(i32rems),
        expectation!(i64remu),
        expectation!(i32remu),
        expectation!(i64and),
        expectation!(i32and),
        expectation!(i64or),
        expectation!(i32or),
        expectation!(i64xor),
        expectation!(i32xor),
        expectation!(i64shl),
        expectation!(i32shl),
        expectation!(i64shrs),
        expectation!(i32shrs),
        expectation!(i64shru),
        expectation!(i32shru),
        expectation!(i64rotl),
        expectation!(i32rotl),
        expectation!(i64rotr),
        expectation!(i32rotr),
    ];

    check_expectations(&expectations)
}

/// Check that the weights of syscalls are within the expected range
pub fn check_syscall_weights<T: pallet_gear::Config>(
    weights: SyscallWeights<T>,
    expected: SyscallWeights<T>,
) -> Result<(), Vec<String>> {
    macro_rules! expectation {
        ($inst_name:ident) => {
            WeightExpectation::new(
                weights.$inst_name.ref_time(),
                expected.$inst_name.ref_time(),
                SYSCALL_SPREAD,
                stringify!($inst_name),
            )
        };
    }

    let expectations = vec![
        expectation!(alloc),
        expectation!(free),
        expectation!(free_range),
        expectation!(free_range_per_page),
        expectation!(gr_reserve_gas),
        expectation!(gr_unreserve_gas),
        expectation!(gr_system_reserve_gas),
        expectation!(gr_gas_available),
        expectation!(gr_message_id),
        expectation!(gr_program_id),
        expectation!(gr_source),
        expectation!(gr_value),
        expectation!(gr_value_available),
        expectation!(gr_size),
        expectation!(gr_read),
        expectation!(gr_read_per_byte),
        expectation!(gr_env_vars),
        expectation!(gr_block_height),
        expectation!(gr_block_timestamp),
        expectation!(gr_random),
        expectation!(gr_reply_deposit),
        expectation!(gr_send),
        expectation!(gr_send_per_byte),
        expectation!(gr_send_wgas),
        expectation!(gr_send_wgas_per_byte),
        expectation!(gr_send_init),
        expectation!(gr_send_push),
        expectation!(gr_send_push_per_byte),
        expectation!(gr_send_commit),
        expectation!(gr_send_commit_wgas),
        expectation!(gr_reservation_send),
        expectation!(gr_reservation_send_per_byte),
        expectation!(gr_reservation_send_commit),
        expectation!(gr_reply_commit),
        expectation!(gr_reply_commit_wgas),
        expectation!(gr_reservation_reply),
        expectation!(gr_reservation_reply_per_byte),
        expectation!(gr_reservation_reply_commit),
        expectation!(gr_reply_push),
        expectation!(gr_reply),
        expectation!(gr_reply_per_byte),
        expectation!(gr_reply_wgas),
        expectation!(gr_reply_wgas_per_byte),
        expectation!(gr_reply_push_per_byte),
        expectation!(gr_reply_to),
        expectation!(gr_signal_code),
        expectation!(gr_signal_from),
        expectation!(gr_reply_input),
        expectation!(gr_reply_input_wgas),
        expectation!(gr_reply_push_input),
        expectation!(gr_reply_push_input_per_byte),
        expectation!(gr_send_input),
        expectation!(gr_send_input_wgas),
        expectation!(gr_send_push_input),
        expectation!(gr_send_push_input_per_byte),
        expectation!(gr_debug),
        expectation!(gr_debug_per_byte),
        expectation!(gr_reply_code),
        expectation!(gr_exit),
        expectation!(gr_leave),
        expectation!(gr_wait),
        expectation!(gr_wait_for),
        expectation!(gr_wait_up_to),
        expectation!(gr_wake),
        expectation!(gr_create_program),
        expectation!(gr_create_program_payload_per_byte),
        expectation!(gr_create_program_salt_per_byte),
        expectation!(gr_create_program_wgas),
        expectation!(gr_create_program_wgas_payload_per_byte),
        expectation!(gr_create_program_wgas_salt_per_byte),
    ];

    check_expectations(&expectations)
}

/// Check that the lazy-pages costs are within the expected range
pub fn check_lazy_pages_costs(
    lazy_pages_costs: LazyPagesCosts,
    expected_lazy_pages_costs: LazyPagesCosts,
) -> Result<(), Vec<String>> {
    macro_rules! expectation {
        ($inst_name:ident) => {
            WeightExpectation::new(
                lazy_pages_costs.$inst_name.cost_for_one(),
                expected_lazy_pages_costs.$inst_name.cost_for_one(),
                PAGES_SPREAD,
                stringify!($inst_name),
            )
        };
    }

    let expectations = vec![
        expectation!(signal_read),
        expectation!(signal_write),
        expectation!(signal_write_after_read),
        expectation!(host_func_read),
        expectation!(host_func_write),
        expectation!(host_func_write_after_read),
        expectation!(load_page_storage_data),
    ];

    check_expectations(&expectations)
}

/// Memory pages access costs.
pub struct PagesCosts {
    pub load_page_data: CostOf<GearPagesAmount>,
    pub upload_page_data: CostOf<GearPagesAmount>,
    pub mem_grow: CostOf<GearPagesAmount>,
    pub mem_grow_per_page: CostOf<GearPagesAmount>,
    pub parachain_read_heuristic: CostOf<GearPagesAmount>,
}

impl<T: pallet_gear::Config> From<MemoryWeights<T>> for PagesCosts {
    fn from(val: MemoryWeights<T>) -> Self {
        Self {
            load_page_data: val.load_page_data.ref_time().into(),
            upload_page_data: val.upload_page_data.ref_time().into(),
            mem_grow: val.mem_grow.ref_time().into(),
            mem_grow_per_page: val.mem_grow_per_page.ref_time().into(),
            parachain_read_heuristic: val.parachain_read_heuristic.ref_time().into(),
        }
    }
}

/// Check that the pages costs are within the expected range
pub fn check_pages_costs(
    page_costs: PagesCosts,
    expected_page_costs: PagesCosts,
) -> Result<(), Vec<String>> {
    macro_rules! expectation {
        ($inst_name:ident) => {
            WeightExpectation::new(
                page_costs.$inst_name.cost_for_one(),
                expected_page_costs.$inst_name.cost_for_one(),
                PAGES_SPREAD,
                stringify!($inst_name),
            )
        };
    }

    let expectations = vec![
        expectation!(load_page_data),
        expectation!(upload_page_data),
        expectation!(mem_grow),
        expectation!(mem_grow_per_page),
        expectation!(parachain_read_heuristic),
    ];

    check_expectations(&expectations)
}
