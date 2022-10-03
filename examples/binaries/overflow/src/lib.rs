#![no_std]

use gstd::exec;

#[no_mangle]
extern "C" fn handle() {
    let bh = exec::block_height();
    gstd::debug!("Block height: {}", bh);
}

#[cfg(test)]
mod tests {
    use gtest::{Program, System};
    #[test]
    fn overflow() {
        let sys = System::new();
        sys.init_logger();

        let prog = Program::current(&sys);
        let res = prog.send_bytes(42, "INIT");
        assert!(res.log().is_empty());

        sys.spend_blocks(u32::MAX / 2 + 1);

        let res = prog.send_bytes(42, "HANDLE");
        assert!(res.main_failed());
    }
}
