use crate::{Interval, IntervalsTree, Numerated};
use alloc::{collections::BTreeSet, fmt::Debug, vec::Vec};

#[derive(Debug)]
pub enum TreeAction<T> {
    Insert(Interval<T>),
    Remove(Interval<T>),
    Voids(Interval<T>),
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
        }
        assert_eq!(expected, tree.points_iter().collect());
    }
}
