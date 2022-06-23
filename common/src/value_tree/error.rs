pub trait Error {
    fn node_already_exists() -> Self;

    fn parent_is_lost() -> Self;

    fn parent_has_no_children() -> Self;

    fn node_not_found() -> Self;

    fn node_was_consumed() -> Self;

    fn insufficient_balance() -> Self;

    fn forbidden() -> Self;
}
