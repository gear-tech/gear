pub trait Counted {
    type Length: Default + PartialEq;

    fn len() -> Self::Length;

    fn is_empty() -> bool {
        Self::len() == Default::default()
    }
}
