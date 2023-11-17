use crate::Program;
use gstd::{any::Any, debug, msg, prelude::*, String, Vec as StdVec};

#[derive(Default)]
pub(crate) struct Vec(StdVec<String>);

impl Program for Vec {
    fn init(_: Box<dyn Any>) -> Self {
        Self::default()
    }

    fn handle(&mut self) {
        let size = msg::load::<i32>().expect("Failed to load `i32`") as usize;

        let request = format!("Request: size = {size}");

        debug!("{request}");
        self.0.push(request);

        let vec = vec![42u8; size];
        let last_idx = size - 1;

        debug!("vec.len() = {:?}", vec.len());
        debug!(
            "vec[{last_idx}]: {:p} -> {:#04x}",
            &vec[last_idx], vec[last_idx]
        );

        msg::reply(size as i32, 0).expect("Failed to send reply");

        // The test idea is to allocate two wasm pages and check this allocation,
        // so we must skip `v` destruction.
        core::mem::forget(vec);

        let requests_amount = self.0.len();
        debug!("Total requests amount: {requests_amount}");

        self.0.iter().for_each(|log| debug!("{log}"));
    }
}
