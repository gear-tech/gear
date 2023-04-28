use crate::Command;
use core::{
    future::Future,
    pin::Pin,
    ptr,
    task::{Context, RawWaker, RawWakerVTable, Waker},
};
use gstd::{lock::RwLock, msg, prelude::*, ActorId};

static mut DESTINATION: ActorId = ActorId::zero();
static RW_LOCK: RwLock<u32> = RwLock::new(0);

async fn ping() {
    msg::send_bytes_for_reply(unsafe { DESTINATION }, "PING", 0)
        .expect("Failed to send message")
        .await
        .expect("Received error reply");
}

#[no_mangle]
extern "C" fn init() {
    let destination = msg::load().expect("Failed to load destination");
    unsafe { DESTINATION = destination };
}

#[gstd::async_main]
async fn main() {
    if let Ok(command) = msg::load() {
        match command {
            Command::Get => {
                let value = RW_LOCK.read().await;
                msg::reply(*value, 0).expect("Failed to send reply");
            }
            Command::Inc => {
                let mut value = RW_LOCK.write().await;
                *value += 1;
            }
            Command::PingGet => {
                ping().await;
                let value = RW_LOCK.read().await;
                msg::reply(*value, 0).expect("Failed to send reply");
            }
            Command::IncPing => {
                let mut value = RW_LOCK.write().await;
                *value += 1;
                ping().await;
            }
            Command::GetPing => {
                let value = RW_LOCK.read().await;
                ping().await;
                msg::reply(*value, 0).expect("Failed to send reply");
            }
            Command::CheckReaders => {
                let mut storage = Vec::with_capacity(RwLock::<u32>::READERS_LIMIT as usize);

                for _ in 0..RwLock::<u32>::READERS_LIMIT {
                    storage.push(RW_LOCK.read().await);
                }

                let waker = unsafe { Waker::from_raw(clone_waker(ptr::null())) };
                let mut cx = Context::from_waker(&waker);

                // Read future just for extra testing
                let mut wf = RW_LOCK.write();

                assert!(
                    !Pin::new(&mut wf).poll(&mut cx).is_ready(),
                    "Ready, but shouldn't"
                );

                let mut rf = RW_LOCK.read();

                assert!(
                    !Pin::new(&mut rf).poll(&mut cx).is_ready(),
                    "Ready, but shouldn't"
                );

                // Drop of single reader.
                storage.pop();

                // Read future just for extra testing
                assert!(
                    !Pin::new(&mut wf).poll(&mut cx).is_ready(),
                    "Ready, but shouldn't"
                );
                assert!(
                    Pin::new(&mut rf).poll(&mut cx).is_ready(),
                    "Not ready, but shouldn't"
                );

                let value = rf.await;
                msg::reply(*value, 0).expect("Failed to send reply");
            }
        }
    } else {
        let _write = RW_LOCK.write().await;
        RW_LOCK.read().await;
    }
}

unsafe fn clone_waker(ptr: *const ()) -> RawWaker {
    RawWaker::new(
        ptr,
        &RawWakerVTable::new(clone_waker, |_| {}, |_| {}, |_| {}),
    )
}
