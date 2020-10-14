
mod msg {
    mod sys {
        extern "C" {
            pub fn send(program: i64, data_ptr: *const u8, data_len: usize);
            pub fn size() -> usize;
            pub fn read(at: usize, len: usize, dest: *mut u8);
        }
    }

    pub fn load() -> Vec<u8> {
        unsafe {
            let message_size = sys::size();
            let mut data = Vec::with_capacity(message_size);
            data.set_len(message_size);
            sys::read(0, message_size, data.as_mut_ptr() as _);
            data
        }
    }

    pub fn send(program: u64, payload: &[u8]) {
        unsafe {
            sys::send(program as _, payload.as_ptr(), payload.len())
        }
    }
}


#[no_mangle]
pub unsafe extern "C" fn handle() {
    assert_eq!(msg::load().len(), 0);

    msg::send(0, &[0u8]);
}

fn main() {
}
