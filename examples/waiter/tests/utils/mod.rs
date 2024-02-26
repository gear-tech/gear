use gstd::errors::{ErrorReplyReason, ReplyCode, SimpleExecutionError};
use gtest::RunResult;

pub const USER_ID: u64 = 10;

#[track_caller]
pub fn assert_panicked(result: &RunResult, panic_msg: &str) {
    assert_eq!(result.log().len(), 1);
    assert!(matches!(
        result.log()[0].reply_code(),
        Some(ReplyCode::Error(ErrorReplyReason::Execution(
            SimpleExecutionError::UserspacePanic
        )))
    ));
    let payload = String::from_utf8(result.log()[0].payload().into())
        .expect("Unable to decode panic message");
    assert!(payload.contains(&format!("panicked with '{panic_msg}'")));
}
