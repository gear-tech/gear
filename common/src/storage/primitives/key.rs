pub trait KeyFor {
    type Key;
    type Value;

    fn key_for(value: &Self::Value) -> Self::Key;
}
