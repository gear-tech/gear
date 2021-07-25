# Gear Examples

## PING-PONG

Gear is very easy to write code for!

Here is a minimal program for a classic ping-pong contract:

```rust
use gstd::{ext, msg};

#[no_mangle]
pub unsafe extern "C" fn handle() {
    let new_msg = String::from_utf8(msg::load()).expect("Invalid message: should be utf-8");

    if &new_msg == "PING" {
        msg::send(msg::source(), b"PONG", u64::MAX);
    }
}

#[no_mangle]
pub unsafe extern "C" fn init() {}
```

It will just send `PONG` back to the original sender (this can be you!)
