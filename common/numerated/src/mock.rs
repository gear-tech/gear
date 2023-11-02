use crate::{Bound, BoundValue, Interval, IntervalsTree, Numerated, One, Zero};
use alloc::{collections::BTreeSet, fmt::Debug, vec::Vec};

pub fn test_numerated<T>(x: T, y: T)
where
    T: Numerated + Debug,
    T::N: Debug,
{
    assert_eq!(x.add_if_between(T::N::one(), y), x.inc_if_lt(y));
    assert_eq!(x.sub_if_between(T::N::one(), y), x.dec_if_gt(y));
    assert_eq!(y.add_if_between(T::N::one(), x), y.inc_if_lt(x));
    assert_eq!(y.sub_if_between(T::N::one(), x), y.dec_if_gt(x));

    assert_eq!(x.add_if_between(T::N::zero(), y), Some(x));
    assert_eq!(x.sub_if_between(T::N::zero(), y), Some(x));
    assert_eq!(y.add_if_between(T::N::zero(), x), Some(y));
    assert_eq!(y.sub_if_between(T::N::zero(), x), Some(y));

    let (x, y) = (x.min(y), x.max(y));
    if x == y {
        assert_eq!(x.inc_if_lt(y), None);
        assert_eq!(x.dec_if_gt(y), None);
        assert_eq!(x.distance(y), Some(T::N::zero()));
    } else {
        assert!(x.inc_if_lt(y).is_some());
        assert!(x.dec_if_gt(y).is_none());
        assert!(y.inc_if_lt(y).is_none());
        assert!(y.dec_if_gt(x).is_some());
        assert!(x.distance(y).is_none());
        let d = y.distance(x).unwrap();
        assert_eq!(x.add_if_between(d, y), Some(y));
        assert_eq!(y.sub_if_between(d, x), Some(x));
    }
}

#[derive(Debug)]
pub enum IntervalAction<T: Numerated> {
    Correct(T::B, T::B),
    Incorrect(T::B, T::B),
}

pub fn test_interval<T>(action: IntervalAction<T>)
where
    T: Numerated + Debug,
    T::B: Debug,
{
    log::debug!("{:?}", action);
    match action {
        IntervalAction::Incorrect(start, end) => {
            assert!(Interval::<T>::new(start, end).is_none());
            assert!(Interval::<T>::try_from(start..end).is_err());
            assert!(Interval::<T>::try_from((start, end)).is_err());
        }
        IntervalAction::Correct(start, end) => {
            let i = Interval::<T>::new(start, end).unwrap();
            if start.get() == end.get() {
                assert!(i.is_empty());
                assert_eq!(i.into_range_inclusive(), None);
                assert_eq!(i.into_inner(), None);
                assert_eq!(i.into_not_empty(), None);
            } else {
                assert_eq!(i.start(), start.get().unwrap());
                assert!(!i.is_empty());
                let i = i.into_not_empty().unwrap();
                match end.unbound() {
                    BoundValue::Value(e) => assert_eq!(i.end(), e.dec_if_gt(i.start()).unwrap()),
                    BoundValue::Upper(e) => assert_eq!(i.end(), e),
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum TreeAction<T> {
    Insert(Interval<T>),
    Remove(Interval<T>),
    Voids(Interval<T>),
    AndNotIterator(BTreeSet<T>),
}

fn btree_set_voids<T: Numerated>(set: &BTreeSet<T>, interval: Interval<T>) -> BTreeSet<T> {
    interval.filter(|p| !set.contains(p)).collect()
}

pub fn test_tree<T: Numerated + Debug>(initial: BTreeSet<T>, actions: Vec<TreeAction<T>>) {
    let mut tree: IntervalsTree<T> = initial.iter().collect();
    let mut expected: BTreeSet<T> = tree.points_iter().collect();
    assert_eq!(expected, initial);

    for action in actions {
        log::debug!("{:?}", action);
        match action {
            TreeAction::Insert(interval) => {
                tree.remove(interval);
                interval.for_each(|i| {
                    expected.remove(&i);
                });
            }
            TreeAction::Remove(interval) => {
                tree.insert(interval);
                expected.extend(interval);
            }
            TreeAction::Voids(interval) => {
                let voids: BTreeSet<T> = tree.voids(interval).flat_map(|i| i.iter()).collect();
                assert_eq!(voids, btree_set_voids(&expected, interval));
            }
            TreeAction::AndNotIterator(x) => {
                let y = x.iter().collect();
                let z: BTreeSet<T> = tree.and_not_iter(&y).flat_map(|i| i.iter()).collect();
                assert_eq!(z, expected.difference(&x).copied().collect());
            }
        }
        assert_eq!(expected, tree.points_iter().collect());
    }
}
