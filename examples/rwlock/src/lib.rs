#![no_std]

use core::{
    future::Future,
    pin::Pin,
    ptr,
    task::{Context, RawWaker, RawWakerVTable, Waker},
};
use gstd::{exec, msg, prelude::*, ProgramId};
use gstd_async::{msg as msg_async, rwlock::RwLockReadGuard};

static mut PING_DEST: ProgramId = ProgramId([0u8; 32]);
static RWLOCK: gstd_async::rwlock::RwLock<u32> = gstd_async::rwlock::RwLock::new(0);

const GAS_LIMIT: u64 = 1_000_000_000;

#[no_mangle]
pub unsafe extern "C" fn init() {
    let dest = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");
    PING_DEST = ProgramId::from_slice(&hex::decode(dest).expect("Invalid hex"));
}

#[gstd_async::main]
async fn main() {
    let message = String::from_utf8(msg::load_bytes()).expect("Invalid message: should be utf-8");

    match message.as_ref() {
        "get" => {
            msg::reply(*RWLOCK.read().await, exec::gas_available() - GAS_LIMIT, 0);
        }
        "inc" => {
            let mut val = RWLOCK.write().await;
            *val += 1;
        }
        "ping&get" => {
            let _ =
                msg_async::send_and_wait_for_reply(unsafe { PING_DEST }, b"PING", GAS_LIMIT * 2, 0)
                    .await
                    .expect("Error in async message processing");
            msg::reply(*RWLOCK.read().await, exec::gas_available() - GAS_LIMIT, 0);
        }
        "inc&ping" => {
            let mut val = RWLOCK.write().await;
            *val += 1;
            let _ = msg_async::send_and_wait_for_reply(
                unsafe { PING_DEST },
                b"PING",
                exec::gas_available() - GAS_LIMIT,
                0,
            )
            .await
            .expect("Error in async message processing");
        }
        "get&ping" => {
            let val = RWLOCK.read().await;
            let _ = msg_async::send_and_wait_for_reply(unsafe { PING_DEST }, b"PING", GAS_LIMIT, 0)
                .await
                .expect("Error in async message processing");
            msg::reply(*val, exec::gas_available() - GAS_LIMIT, 0);
        }
        "check readers" => {
            let mut storage: Vec<RwLockReadGuard<u32>> = Vec::new();
            for _ in 0..32 {
                storage.push(RWLOCK.read().await);
            }

            let waker = unsafe {
                Waker::from_raw(RawWaker::new(
                    ptr::null(),
                    &RawWakerVTable::new(clone_waker, wake, wake_by_ref, drop_waker),
                ))
            };
            let mut cx = Context::from_waker(&waker);

            // Read future just for extra testing
            let mut write_future = RWLOCK.write();

            if Pin::new(&mut write_future).poll(&mut cx).is_ready() {
                panic!("I am ready, but I should't");
            }
            //

            let mut another_future = RWLOCK.read();

            if Pin::new(&mut another_future).poll(&mut cx).is_ready() {
                panic!("I am ready, but I should't");
            }

            storage.pop();

            // Read future just for extra testing
            if Pin::new(&mut write_future).poll(&mut cx).is_ready() {
                panic!("I am ready, but I should't");
            }
            //

            if !Pin::new(&mut another_future).poll(&mut cx).is_ready() {
                panic!("I am not ready, but I should be");
            }

            let val = another_future.await;

            msg::reply(*val, exec::gas_available() - GAS_LIMIT, 0);
        }
        _ => {
            let _write = RWLOCK.write().await;
            RWLOCK.read().await;
        }
    }
}

unsafe fn clone_waker(ptr: *const ()) -> RawWaker {
    RawWaker::new(
        ptr,
        &RawWakerVTable::new(clone_waker, wake, wake_by_ref, drop_waker),
    )
}
unsafe fn wake(_ptr: *const ()) {}
unsafe fn wake_by_ref(_ptr: *const ()) {}
unsafe fn drop_waker(_ptr: *const ()) {}
