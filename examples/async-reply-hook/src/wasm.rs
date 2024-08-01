use gstd::{critical, debug, exec, msg, prelude::*, ActorId};

#[gstd::async_main]
async fn main() {
    let source = msg::source();

    // Case 1: Message without reply hook
    let m1 = gstd::msg::send_bytes_for_reply(source, b"for_reply_1", 0, 0)
        .expect("Failed to send message");

    // Case 2: Message with reply hook but we don't reply to it
    let m2 = gstd::msg::send_bytes_for_reply(source, b"for_reply_2", 0, 1_000_000_000)
        .expect("Failed to send message")
        .up_to(Some(5))
        .expect("Failed to set timeout")
        .handle_reply(|| {
            // This should be called in gas / 100 blocks, but the program exits by that time
            unreachable!("This should not be called");
        })
        .expect("Failed to set reply hook");

    // Case 3: Message with reply hook and we reply to it
    let for_reply_3 = gstd::rc::Rc::new(core::cell::RefCell::new(false));
    let for_reply_3_clone = for_reply_3.clone();
    let m3 = gstd::msg::send_bytes_for_reply(source, b"for_reply_3", 0, 1_000_000_000)
        .expect("Failed to send message")
        .handle_reply(move || {
            debug!("reply message_id: {:?}", msg::id());
            debug!("reply payload: {:?}", msg::load_bytes());

            assert_eq!(msg::load_bytes().unwrap(), [3]);

            msg::send_bytes(msg::source(), b"saw_reply_3", 0);
            for_reply_3_clone.replace(true);
        })
        .expect("Failed to set reply hook");

    // Case 4: We reply to message after timeout
    let m4 = gstd::msg::send_bytes_for_reply(source, b"for_reply_4", 0, 1_000_000_000)
        .expect("Failed to send message")
        .up_to(Some(5))
        .expect("Failed to set timeout")
        .handle_reply(|| {
            debug!("reply message_id: {:?}", msg::id());
            debug!("reply payload: {:?}", msg::load_bytes());

            assert_eq!(msg::load_bytes().unwrap(), [4]);

            msg::send_bytes(msg::source(), b"saw_reply_4", 0);
        })
        .expect("Failed to set reply hook");

    m1.await.expect("Received error reply");

    assert_eq!(
        m2.await.expect_err("Should receive timeout"),
        gstd::errors::Error::Timeout(8, 8)
    );

    m3.await.expect("Received error reply");
    // check for_reply_3 handle_reply executed
    assert!(for_reply_3.replace(false));

    assert_eq!(
        m4.await.expect_err("Should receive timeout"),
        gstd::errors::Error::Timeout(8, 8)
    );

    msg::send_bytes(source, b"completed", 0);
}
