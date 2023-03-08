#![no_std]

use core::{
    future::Future,
    pin::Pin,
    ptr,
    task::{Context, RawWaker, RawWakerVTable, Waker},
};
use gstd::{lock::RwLock, msg, prelude::*, ActorId};

static mut PING_DEST: ActorId = ActorId::new([0u8; 32]);
static RWLOCK: RwLock<u32> = RwLock::new(0);

#[no_mangle]
extern "C" fn init() {
    let dest = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
        .expect("Invalid message: should be utf-8");
    unsafe {
        PING_DEST = ActorId::from_slice(&hex::decode(dest).expect("Invalid hex"))
            .expect("Unable to create ActorId")
    };
}

#[gstd::async_main]
async fn main() {
    let message = String::from_utf8(msg::load_bytes().expect("Failed to load payload bytes"))
        .expect("Invalid message: should be utf-8");

    match message.as_ref() {
        "get" => {
            msg::reply(*RWLOCK.read().await, 0).unwrap();
        }
        "inc" => {
            let mut val = RWLOCK.write().await;
            *val += 1;
        }
        "ping&get" => {
            let _ = msg::send_bytes_for_reply(unsafe { PING_DEST }, b"PING", 0)
                .unwrap()
                .await
                .expect("Error in async message processing");
            msg::reply(*RWLOCK.read().await, 0).unwrap();
        }
        "inc&ping" => {
            let mut val = RWLOCK.write().await;
            *val += 1;
            let _ = msg::send_bytes_for_reply(unsafe { PING_DEST }, b"PING", 0)
                .unwrap()
                .await
                .expect("Error in async message processing");
        }
        "get&ping" => {
            let val = RWLOCK.read().await;
            let _ = msg::send_bytes_for_reply(unsafe { PING_DEST }, b"PING", 0)
                .unwrap()
                .await
                .expect("Error in async message processing");
            msg::reply(*val, 0).unwrap();
        }
        "check readers" => {
            let mut storage = Vec::new();
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

            msg::reply(*val, 0).unwrap();
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
