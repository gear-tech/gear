use crate::Command;

use gstd::{exec, msg};

#[no_mangle]
unsafe extern "C" fn handle() {
    let cmd: Command = msg::load().unwrap();

    match cmd {
        Command::Wait => exec::wait(),
        Command::WaitFor(duration) => exec::wait_for(duration),
        Command::WaitNoMore(duration) => exec::wait_up_to(duration),
    }
}
