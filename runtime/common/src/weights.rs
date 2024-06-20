use gear_core::{costs::CostOf, pages::GearPagesAmount};
use gear_lazy_pages_common::LazyPagesCosts;
use pallet_gear::{InstructionWeights, MemoryWeights, SyscallWeights};

const INSTRUCTIONS_SPREAD: u8 = 50;
const SYSCALL_SPREAD: u8 = 10;
const PAGES_SPREAD: u8 = 10;

#[track_caller]
fn check_spreading(weight: u64, expected: u64, spread: u8) {
    let left = expected - expected * spread as u64 / 100;
    let right = expected + expected * spread as u64 / 100;

    assert!(
        left <= weight && weight <= right,
        "Weight is {weight} ps. Expected weight is {expected} ps. {spread}% spread interval: [{left} ps, {right} ps]"
    );
}

#[track_caller]
fn check_instruction_weight(weight: u32, expected: u32) {
    check_spreading(weight.into(), expected.into(), INSTRUCTIONS_SPREAD);
}

#[track_caller]
fn check_syscall_weight(weight: u64, expected: u64) {
    check_spreading(weight, expected, SYSCALL_SPREAD);
}

#[track_caller]
fn check_pages_weight(weight: u64, expected: u64) {
    check_spreading(weight, expected, PAGES_SPREAD);
}

/// Check that the weights of instructions are within the expected range
pub fn check_instructions_weights<T: pallet_gear::Config>(
    weights: InstructionWeights<T>,
    expected: InstructionWeights<T>,
) {
    check_instruction_weight(weights.i64const, expected.i64const);
    check_instruction_weight(weights.i64load, expected.i64load);
    check_instruction_weight(weights.i32load, expected.i32load);
    check_instruction_weight(weights.i64store, expected.i64store);
    check_instruction_weight(weights.i32store, expected.i32store);
    check_instruction_weight(weights.select, expected.select);
    check_instruction_weight(weights.r#if, expected.r#if);
    check_instruction_weight(weights.br, expected.br);
    check_instruction_weight(weights.br_if, expected.br_if);
    check_instruction_weight(weights.br_table, expected.br_table);
    check_instruction_weight(weights.br_table_per_entry, expected.br_table_per_entry);
    check_instruction_weight(weights.call, expected.call);
    check_instruction_weight(weights.call_indirect, expected.call_indirect);
    check_instruction_weight(
        weights.call_indirect_per_param,
        expected.call_indirect_per_param,
    );
    check_instruction_weight(weights.call_per_local, expected.call_per_local);
    check_instruction_weight(weights.local_get, expected.local_get);
    check_instruction_weight(weights.local_set, expected.local_set);
    check_instruction_weight(weights.local_tee, expected.local_tee);
    check_instruction_weight(weights.global_get, expected.global_get);
    check_instruction_weight(weights.global_set, expected.global_set);
    check_instruction_weight(weights.memory_current, expected.memory_current);
    check_instruction_weight(weights.i64clz, expected.i64clz);
    check_instruction_weight(weights.i32clz, expected.i32clz);
    check_instruction_weight(weights.i64ctz, expected.i64ctz);
    check_instruction_weight(weights.i32ctz, expected.i32ctz);
    check_instruction_weight(weights.i64popcnt, expected.i64popcnt);
    check_instruction_weight(weights.i32popcnt, expected.i32popcnt);
    check_instruction_weight(weights.i64eqz, expected.i64eqz);
    check_instruction_weight(weights.i32eqz, expected.i32eqz);
    check_instruction_weight(weights.i32extend8s, expected.i32extend8s);
    check_instruction_weight(weights.i32extend16s, expected.i32extend16s);
    check_instruction_weight(weights.i64extend8s, expected.i64extend8s);
    check_instruction_weight(weights.i64extend16s, expected.i64extend16s);
    check_instruction_weight(weights.i64extend32s, expected.i64extend32s);
    check_instruction_weight(weights.i64extendsi32, expected.i64extendsi32);
    check_instruction_weight(weights.i64extendui32, expected.i64extendui32);
    check_instruction_weight(weights.i32wrapi64, expected.i32wrapi64);
    check_instruction_weight(weights.i64eq, expected.i64eq);
    check_instruction_weight(weights.i32eq, expected.i32eq);
    check_instruction_weight(weights.i64ne, expected.i64ne);
    check_instruction_weight(weights.i32ne, expected.i32ne);
    check_instruction_weight(weights.i64lts, expected.i64lts);
    check_instruction_weight(weights.i32lts, expected.i32lts);
    check_instruction_weight(weights.i64ltu, expected.i64ltu);
    check_instruction_weight(weights.i32ltu, expected.i32ltu);
    check_instruction_weight(weights.i64gts, expected.i64gts);
    check_instruction_weight(weights.i32gts, expected.i32gts);
    check_instruction_weight(weights.i64gtu, expected.i64gtu);
    check_instruction_weight(weights.i32gtu, expected.i32gtu);
    check_instruction_weight(weights.i64les, expected.i64les);
    check_instruction_weight(weights.i32les, expected.i32les);
    check_instruction_weight(weights.i64leu, expected.i64leu);
    check_instruction_weight(weights.i32leu, expected.i32leu);
    check_instruction_weight(weights.i64ges, expected.i64ges);
    check_instruction_weight(weights.i32ges, expected.i32ges);
    check_instruction_weight(weights.i64geu, expected.i64geu);
    check_instruction_weight(weights.i32geu, expected.i32geu);
    check_instruction_weight(weights.i64add, expected.i64add);
    check_instruction_weight(weights.i32add, expected.i32add);
    check_instruction_weight(weights.i64sub, expected.i64sub);
    check_instruction_weight(weights.i32sub, expected.i32sub);
    check_instruction_weight(weights.i64mul, expected.i64mul);
    check_instruction_weight(weights.i32mul, expected.i32mul);
    check_instruction_weight(weights.i64divs, expected.i64divs);
    check_instruction_weight(weights.i32divs, expected.i32divs);
    check_instruction_weight(weights.i64divu, expected.i64divu);
    check_instruction_weight(weights.i32divu, expected.i32divu);
    check_instruction_weight(weights.i64rems, expected.i64rems);
    check_instruction_weight(weights.i32rems, expected.i32rems);
    check_instruction_weight(weights.i64remu, expected.i64remu);
    check_instruction_weight(weights.i32remu, expected.i32remu);
    check_instruction_weight(weights.i64and, expected.i64and);
    check_instruction_weight(weights.i32and, expected.i32and);
    check_instruction_weight(weights.i64or, expected.i64or);
    check_instruction_weight(weights.i32or, expected.i32or);
    check_instruction_weight(weights.i64xor, expected.i64xor);
    check_instruction_weight(weights.i32xor, expected.i32xor);
    check_instruction_weight(weights.i64shl, expected.i64shl);
    check_instruction_weight(weights.i32shl, expected.i32shl);
    check_instruction_weight(weights.i64shrs, expected.i64shrs);
    check_instruction_weight(weights.i32shrs, expected.i32shrs);
    check_instruction_weight(weights.i64shru, expected.i64shru);
    check_instruction_weight(weights.i32shru, expected.i32shru);
    check_instruction_weight(weights.i64rotl, expected.i64rotl);
    check_instruction_weight(weights.i32rotl, expected.i32rotl);
    check_instruction_weight(weights.i64rotr, expected.i64rotr);
    check_instruction_weight(weights.i32rotr, expected.i32rotr);
}

/// Check that the weights of syscalls are within the expected range
pub fn check_syscall_weights<T: pallet_gear::Config>(
    weights: SyscallWeights<T>,
    expected: SyscallWeights<T>,
) {
    macro_rules! check {
        ($inst_name:ident) => {
            check_syscall_weight(
                weights.$inst_name.ref_time(),
                expected.$inst_name.ref_time(),
            );
        };
    }

    check!(alloc);
    check!(free);
    check!(free_range);
    check!(free_range_per_page);
    check!(gr_reserve_gas);
    check!(gr_unreserve_gas);
    check!(gr_system_reserve_gas);
    check!(gr_gas_available);
    check!(gr_message_id);
    check!(gr_program_id);
    check!(gr_source);
    check!(gr_value);
    check!(gr_value_available);
    check!(gr_size);
    check!(gr_read);
    check!(gr_read_per_byte);
    check!(gr_env_vars);
    check!(gr_block_height);
    check!(gr_block_timestamp);
    check!(gr_random);
    check!(gr_reply_deposit);
    check!(gr_send);
    check!(gr_send_per_byte);
    check!(gr_send_wgas);
    check!(gr_send_wgas_per_byte);
    check!(gr_send_init);
    check!(gr_send_push);
    check!(gr_send_push_per_byte);
    check!(gr_send_commit);
    check!(gr_send_commit_wgas);
    check!(gr_reservation_send);
    check!(gr_reservation_send_per_byte);
    check!(gr_reservation_send_commit);
    check!(gr_reply_commit);
    check!(gr_reply_commit_wgas);
    check!(gr_reservation_reply);
    check!(gr_reservation_reply_per_byte);
    check!(gr_reservation_reply_commit);
    check!(gr_reply_push);
    check!(gr_reply);
    check!(gr_reply_per_byte);
    check!(gr_reply_wgas);
    check!(gr_reply_wgas_per_byte);
    check!(gr_reply_push_per_byte);
    check!(gr_reply_to);
    check!(gr_signal_code);
    check!(gr_signal_from);
    check!(gr_reply_input);
    check!(gr_reply_input_wgas);
    check!(gr_reply_push_input);
    check!(gr_reply_push_input_per_byte);
    check!(gr_send_input);
    check!(gr_send_input_wgas);
    check!(gr_send_push_input);
    check!(gr_send_push_input_per_byte);
    check!(gr_debug);
    check!(gr_debug_per_byte);
    check!(gr_reply_code);
    check!(gr_exit);
    check!(gr_leave);
    check!(gr_wait);
    check!(gr_wait_for);
    check!(gr_wait_up_to);
    check!(gr_wake);
    check!(gr_create_program);
    check!(gr_create_program_payload_per_byte);
    check!(gr_create_program_salt_per_byte);
    check!(gr_create_program_wgas);
    check!(gr_create_program_wgas_payload_per_byte);
    check!(gr_create_program_wgas_salt_per_byte);
}

/// Check that the lazy-pages costs are within the expected range
pub fn check_lazy_pages_costs(
    lazy_pages_costs: LazyPagesCosts,
    expected_lazy_pages_costs: LazyPagesCosts,
) {
    check_pages_weight(
        lazy_pages_costs.signal_read.cost_for_one(),
        expected_lazy_pages_costs.signal_read.cost_for_one(),
    );
    check_pages_weight(
        lazy_pages_costs.signal_write.cost_for_one(),
        expected_lazy_pages_costs.signal_write.cost_for_one(),
    );
    check_pages_weight(
        lazy_pages_costs.signal_write_after_read.cost_for_one(),
        expected_lazy_pages_costs
            .signal_write_after_read
            .cost_for_one(),
    );
    check_pages_weight(
        lazy_pages_costs.host_func_read.cost_for_one(),
        expected_lazy_pages_costs.host_func_read.cost_for_one(),
    );
    check_pages_weight(
        lazy_pages_costs.host_func_write.cost_for_one(),
        expected_lazy_pages_costs.host_func_write.cost_for_one(),
    );
    check_pages_weight(
        lazy_pages_costs.host_func_write_after_read.cost_for_one(),
        expected_lazy_pages_costs
            .host_func_write_after_read
            .cost_for_one(),
    );
    check_pages_weight(
        lazy_pages_costs.load_page_storage_data.cost_for_one(),
        expected_lazy_pages_costs
            .load_page_storage_data
            .cost_for_one(),
    );
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
pub fn check_pages_costs(page_costs: PagesCosts, expected_page_costs: PagesCosts) {
    check_pages_weight(
        page_costs.load_page_data.cost_for_one(),
        expected_page_costs.load_page_data.cost_for_one(),
    );

    check_pages_weight(
        page_costs.upload_page_data.cost_for_one(),
        expected_page_costs.upload_page_data.cost_for_one(),
    );

    check_pages_weight(
        page_costs.mem_grow.cost_for_one(),
        expected_page_costs.mem_grow.cost_for_one(),
    );

    check_pages_weight(
        page_costs.mem_grow_per_page.cost_for_one(),
        expected_page_costs.mem_grow_per_page.cost_for_one(),
    );

    check_pages_weight(
        page_costs.parachain_read_heuristic.cost_for_one(),
        expected_page_costs.parachain_read_heuristic.cost_for_one(),
    );
}
