// Auto-generated integration test
// scenario: gas_boundary
// sig: InBlockTransitions_schedule_task_block_height_overflow

use ethexe_runtime_common::InBlockTransitions;
use ethexe_common::{ProgramStates, Schedule, ScheduledTask};
use gprimitives::{ActorId, MessageId};
use core::num::NonZero;

#[test]
fn auto_tester_b556aa218ff2() {
    // schedule_task computes: scheduled_block = block_height + u32::from(in_blocks)
    // Test at block_height = u32::MAX - 1 with in_blocks = 1 → scheduled_block = u32::MAX
    let mut transitions = InBlockTransitions::new(u32::MAX - 1, ProgramStates::default(), Schedule::default());

    let task = ScheduledTask::WakeMessage(ActorId::from([1u8; 32]), MessageId::from([2u8; 32]));
    let in_blocks = NonZero::new(1u32).unwrap();

    let scheduled_at = transitions.schedule_task(in_blocks, task.clone());
    assert_eq!(scheduled_at, u32::MAX, "Expected scheduling at u32::MAX block");

    // Drain at block_height = u32::MAX - 1; the task is in the future (u32::MAX), so not drained
    let drained = transitions.take_actual_tasks();
    assert!(drained.is_empty(), "Task at u32::MAX must not be drained at block_height u32::MAX-1");

    // The schedule now has one entry at u32::MAX — finalize gives us the schedule to reuse
    let finalized = transitions.finalize();
    let schedule = finalized.schedule;

    // Create new transitions at block_height = u32::MAX with the preserved schedule
    let mut transitions2 = InBlockTransitions::new(u32::MAX, ProgramStates::default(), schedule);
    let drained = transitions2.take_actual_tasks();
    assert_eq!(drained.len(), 1, "Task at u32::MAX must be drained at block_height u32::MAX");
    assert_eq!(drained[0], task);
}
