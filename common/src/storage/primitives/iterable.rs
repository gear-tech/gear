pub trait IterableMap {
    type Key;
    type Value;

    fn drain() -> Box<dyn Iterator<Item = (Self::Key, Self::Value)>>;

    fn iter() -> dyn Iterator<Item = (Self::Key, Self::Value)>;

    fn iter_keys() -> dyn Iterator<Item = Self::Key>;

    fn iter_values() -> dyn Iterator<Item = Self::Value>;
}
