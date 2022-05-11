use crate::storage::primitives::ValueStorage;
use core::marker::PhantomData;

pub trait Counter {
    type Value;

    fn decrease();

    fn get() -> Self::Value;

    fn increase();

    fn reset();
}

pub struct CounterImpl<T, VS: ValueStorage<Value = T>>(PhantomData<VS>);

macro_rules! impl_counter {
    ($($t: ty), +) => { $(
        impl<VS: ValueStorage<Value = $t>> Counter for CounterImpl<$t, VS> {
            type Value = VS::Value;

            fn decrease() {
                VS::mutate(|opt_val| {
                    if let Some(val) = opt_val {
                        *val = val.saturating_sub(1);
                    }
                });
            }

            fn get() -> Self::Value {
                VS::get().unwrap_or(0)
            }

            fn increase() {
                VS::mutate(|opt_val| {
                    if let Some(val) = opt_val {
                        *val = val.saturating_add(1);
                    } else {
                        *opt_val = Some(1)
                    }
                });
            }

            fn reset() {
                VS::put(0);
            }
        }
    ) + };
}

impl_counter!(u8, u16, u32, u64, u128);
impl_counter!(i8, i16, i32, i64, i128);
