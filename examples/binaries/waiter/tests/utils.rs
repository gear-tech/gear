use gstd::errors::{ErrorReplyReason, ReplyCode, SimpleExecutionError};
use gtest::RunResult;

pub const USER_ID: u64 = 10;

#[track_caller]
pub fn assert_paniced(result: &RunResult, panic_msg: &str) {
    assert_eq!(result.log().len(), 1);
    assert!(matches!(
        result.log()[0].reply_code(),
        Some(ReplyCode::Error(ErrorReplyReason::Execution(
            SimpleExecutionError::UserspacePanic
        )))
    ));
    let payload = String::from_utf8(result.log()[0].payload().into())
        .expect("Unable to decode panic message")
        .split(',')
        .map(String::from)
        .next()
        .expect("Unable to split panic message");
    assert_eq!(payload, format!("'{}'", panic_msg));
}
