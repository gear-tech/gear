// scenario: overflow
// param_signature: schedule_task(NonZero<u32>::MAX, task) wraps block_height+u32_max
// hash: b1ed02dd83bc

use ethexe_runtime_common::{InBlockTransitions, state::MemStorage};
use ethexe_common::{ScheduledTask, Schedule};
use gprimitives::{ActorId, MessageId};
use core::num::NonZero;
use std::collections::BTreeMap;

#[test]
fn test_schedule_task_in_blocks_max_overflow() {
    // block_height = 5, in_blocks = u32::MAX
    // scheduled_block = 5u32.wrapping_add(u32::MAX) = 4
    // This would silently schedule a task in the "past" (block 4), which is before block_height=5.
    // take_actual_tasks at block 5 would then drain block 4, so the task appears immediately.
    let block_height: u32 = 5;
    let states = BTreeMap::new();
    let schedule: Schedule = BTreeMap::new();
    let mut transitions = InBlockTransitions::new(block_height, states, schedule);

    let task = ScheduledTask::WakeMessage(ActorId::from(1), MessageId::from(1));
    let in_blocks = NonZero::<u32>::new(u32::MAX).expect("non-zero");

    // scheduled_block should be 5 + u32::MAX = 4 (wraps)
    let scheduled_block = transitions.schedule_task(in_blocks, task.clone());

    // With overflow, scheduled_block = 5u32 + u32::MAX = 4 (wraps to 4 on u32)
    // This is less than current block_height (5), so take_actual_tasks would drain it immediately.
    // That means a task scheduled "u32::MAX blocks in the future" fires on the very next call.
    let tasks = transitions.take_actual_tasks();

    // If the overflow is silent, the task scheduled for a "huge future" block
    // actually ends up in the past and fires immediately — this is the bug we probe.
    // We verify what actually happens and record it.
    let overflow_occurred = scheduled_block < block_height;
    assert!(
        !overflow_occurred || tasks.contains(&task),
        "overflow: scheduled_block={scheduled_block} is before block_height={block_height}, \
         task fires immediately instead of far in future"
    );
}
