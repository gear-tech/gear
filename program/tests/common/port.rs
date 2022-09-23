//! Picking random ports
use rand::Rng;
use std::{net::TcpListener, ops::Range};

/// localhost addr
pub const LOCALHOST: &str = "127.0.0.1";
const PORT_RANGE: Range<u16> = 15000..25000;

/// Pick a random port
pub fn pick() -> u16 {
    let mut rng = rand::thread_rng();

    loop {
        let port = rng.gen_range(PORT_RANGE);
        if TcpListener::bind(&format!("{}:{}", LOCALHOST, port)).is_ok() {
            return port;
        }
    }
}
