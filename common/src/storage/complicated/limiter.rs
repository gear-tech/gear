use crate::storage::primitives::ValueStorage;
use core::marker::PhantomData;

pub trait Limiter {
    type Value;

    fn decrease(value: Self::Value);

    fn get() -> Self::Value;

    fn put(value: Self::Value);
}

pub struct LimiterImpl<T, VS: ValueStorage<Value = T>>(PhantomData<VS>);

macro_rules! impl_limiter {
    ($($t: ty), +) => { $(
        impl<VS: ValueStorage<Value = $t>> Limiter for LimiterImpl<$t, VS> {
            type Value = VS::Value;

            fn decrease(value: Self::Value) {
                VS::mutate(|opt_val| {
                    if let Some(val) = opt_val {
                        *val = val.saturating_sub(value);
                    }
                });
            }

            fn get() -> Self::Value {
                VS::get().unwrap_or(0)
            }

            fn put(value: Self::Value) {
                VS::put(value);
            }
        }
    ) + };
}

impl_limiter!(u8, u16, u32, u64, u128);
impl_limiter!(i8, i16, i32, i64, i128);
