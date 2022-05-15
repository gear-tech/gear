//! Storage primitives: Value, Map, DoubleMap, CountedMap, Callback.

mod callback;
mod counted;
mod double_map;
mod iterable;
mod key;
mod map;
mod value;

pub use callback::{Callback, EmptyCallback};
pub use counted::Counted;
pub use double_map::DoubleMapStorage;
pub use iterable::IterableMap;
pub use key::{KeyFor, MailboxKeyGen, QueueKeyGen};
pub use map::MapStorage;
pub use value::ValueStorage;
