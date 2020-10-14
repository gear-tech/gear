
mod msg {
    extern "C" {
        pub fn send(program: i64, data_ptr: *const u8, data_len: usize);
        pub fn size() -> i32;
        pub fn read(at: usize, len: usize, dest: *mut u8);
    }
}


#[no_mangle]
pub unsafe extern "C" fn handle() {
    assert_eq!(msg::size(), 0)
}

fn main() {
}
