#![allow(clippy::reversed_empty_ranges)]

use crate::{
    mock::{self, test_interval, test_numerated, IntervalAction, TreeAction},
    BoundValue, Interval,
};
use alloc::{collections::BTreeSet, vec::Vec};
use proptest::{
    arbitrary::any, prop_oneof, proptest, strategy::Strategy, test_runner::Config as ProptestConfig,
};

fn rand_interval() -> impl Strategy<Value = Interval<i16>> {
    any::<i16>()
        .prop_flat_map(|start| (start..).prop_map(move |end| (start..=end).try_into().unwrap()))
}

fn rand_set() -> impl Strategy<Value = BTreeSet<i16>> {
    proptest::collection::btree_set(any::<i16>(), 0..1000)
}

fn tree_actions() -> impl Strategy<Value = Vec<TreeAction<i16>>> {
    let action = prop_oneof![
        rand_interval().prop_map(TreeAction::Insert),
        rand_interval().prop_map(TreeAction::Remove),
        rand_interval().prop_map(TreeAction::Voids),
        rand_set().prop_map(TreeAction::AndNotIterator),
    ];
    proptest::collection::vec(action, 10..20)
}

fn interval_action() -> impl Strategy<Value = IntervalAction<i16>> {
    let start = any::<Option<i16>>();
    let end = any::<Option<i16>>();
    (start, end).prop_map(|(start, end)| {
        let start: BoundValue<i16> = start.into();
        let end: BoundValue<i16> = end.into();
        match (start, end) {
            (_, BoundValue::Upper(_)) => IntervalAction::Correct(start, end),
            (BoundValue::Value(s), BoundValue::Value(e)) => {
                if s > e {
                    IntervalAction::Incorrect(start, end)
                } else {
                    IntervalAction::Correct(start, end)
                }
            }
            (BoundValue::Upper(_), BoundValue::Value(_)) => IntervalAction::Incorrect(start, end),
        }
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn proptest_numerated(x in any::<i16>(), y in any::<i16>()) {
        test_numerated(x, y);
    }

    #[test]
    fn proptest_interval(action in interval_action()) {
        test_interval(action);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn proptest_tree(actions in tree_actions(), initial in rand_set()) {
        mock::test_tree(initial, actions);
    }
}
