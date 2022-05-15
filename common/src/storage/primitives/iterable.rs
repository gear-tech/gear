pub trait IterableMap<Item> {
    type DrainIter: Iterator<Item = Item>;
    type Iter: Iterator<Item = Item>;

    fn drain() -> Self::DrainIter;
    fn iter() -> Self::Iter;
}
