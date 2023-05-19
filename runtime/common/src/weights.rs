use gear_backend_common::lazy_pages::LazyPagesWeights;
use gear_core_processor::configs::PageCosts;
use pallet_gear::InstructionWeights;

const INSTRUCTIONS_SPREAD: u64 = 50;
const PAGES_SPREAD: u64 = 10;

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
fn check_pages_weight(weight: u64, expected: u64) {
    check_spreading(weight, expected, PAGES_SPREAD);
}

/// Check that the weights of instructions are within the expected range
#[track_caller]
pub fn check_instructions_weights<T: pallet_gear::Config>(
    weights: InstructionWeights<T>,
    expected: InstructionWeights<T>,
) {
    check_instruction_weight(weights.i64const, expected.i64const);
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

/// Check that the weights of page operations are within the expected range
#[track_caller]
pub fn check_pages_weights(
    weights: PageCosts,
    expected_page_costs: PageCosts,
    lazy_pages_weights: LazyPagesWeights,
    expected_lazy_pages_weights: LazyPagesWeights,
) {
    check_pages_weight(
        weights.lazy_pages_signal_read.one(),
        expected_page_costs.lazy_pages_signal_read.one(),
    );
    check_pages_weight(
        weights.lazy_pages_signal_write.one(),
        expected_page_costs.lazy_pages_signal_write.one(),
    );
    check_pages_weight(
        weights.lazy_pages_signal_write_after_read.one(),
        expected_page_costs.lazy_pages_signal_write_after_read.one(),
    );
    check_pages_weight(
        weights.lazy_pages_host_func_read.one(),
        expected_page_costs.lazy_pages_host_func_read.one(),
    );
    check_pages_weight(
        weights.lazy_pages_host_func_write.one(),
        expected_page_costs.lazy_pages_host_func_write.one(),
    );
    check_pages_weight(
        weights.lazy_pages_host_func_write_after_read.one(),
        expected_page_costs
            .lazy_pages_host_func_write_after_read
            .one(),
    );
    check_pages_weight(
        weights.load_page_data.one(),
        expected_page_costs.load_page_data.one(),
    );
    check_pages_weight(
        weights.upload_page_data.one(),
        expected_page_costs.upload_page_data.one(),
    );
    check_pages_weight(
        weights.static_page.one(),
        expected_page_costs.static_page.one(),
    );
    check_pages_weight(weights.mem_grow.one(), expected_page_costs.mem_grow.one());
    check_pages_weight(
        weights.parachain_load_heuristic.one(),
        expected_page_costs.parachain_load_heuristic.one(),
    );

    check_pages_weight(
        lazy_pages_weights.signal_read.one(),
        expected_lazy_pages_weights.signal_read.one(),
    );
    check_pages_weight(
        lazy_pages_weights.signal_write.one(),
        expected_lazy_pages_weights.signal_write.one(),
    );
    check_pages_weight(
        lazy_pages_weights.signal_write_after_read.one(),
        expected_lazy_pages_weights.signal_write_after_read.one(),
    );
    check_pages_weight(
        lazy_pages_weights.host_func_read.one(),
        expected_lazy_pages_weights.host_func_read.one(),
    );
    check_pages_weight(
        lazy_pages_weights.host_func_write.one(),
        expected_lazy_pages_weights.host_func_write.one(),
    );
    check_pages_weight(
        lazy_pages_weights.host_func_write_after_read.one(),
        expected_lazy_pages_weights.host_func_write_after_read.one(),
    );
    check_pages_weight(
        lazy_pages_weights.load_page_storage_data.one(),
        expected_lazy_pages_weights.load_page_storage_data.one(),
    );
}
