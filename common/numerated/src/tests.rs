#![allow(clippy::reversed_empty_ranges)]

use crate::{Interval, IntervalsTree};
use alloc::{vec, vec::Vec};
use core::ops::RangeInclusive;

#[test]
fn test_insert() {
    let mut drops = IntervalsTree::new();
    drops.try_insert(1..=2).unwrap();
    assert_eq!(drops.to_vec(), vec![1..=2]);

    let mut drops = IntervalsTree::new();
    drops.try_insert(-1..=2).unwrap();
    drops.try_insert(4..=5).unwrap();
    assert_eq!(drops.to_vec(), vec![-1..=2, 4..=5]);

    let mut drops = IntervalsTree::new();
    drops.try_insert(-1..=2).unwrap();
    drops.try_insert(3..=4).unwrap();
    assert_eq!(drops.to_vec(), vec![-1..=4]);

    let mut drops = IntervalsTree::new();
    drops.insert(1);
    drops.insert(2);
    assert_eq!(drops.to_vec(), vec![1..=2]);

    let mut drops = IntervalsTree::new();
    drops.try_insert(-1..=3).unwrap();
    drops.try_insert(5..=7).unwrap();
    drops.try_insert(2..=6).unwrap();
    drops.try_insert(7..=7).unwrap();
    drops.try_insert(19..=25).unwrap();
    assert_eq!(drops.to_vec(), vec![-1..=7, 19..=25]);

    let mut drops = IntervalsTree::new();
    drops.try_insert(-1..=3).unwrap();
    drops.try_insert(10..=14).unwrap();
    drops.try_insert(4..=9).unwrap();
    assert_eq!(drops.to_vec(), vec![-1..=14]);

    let mut drops = IntervalsTree::new();
    drops.try_insert(-111..=3).unwrap();
    drops.try_insert(10..=14).unwrap();
    drops.try_insert(3..=10).unwrap();
    assert_eq!(drops.to_vec(), vec![-111..=14]);

    let mut drops = IntervalsTree::new();
    drops.try_insert(i32::MIN..=10).unwrap();
    drops.try_insert(3..=4).unwrap();
    assert_eq!(drops.to_vec(), vec![i32::MIN..=10]);

    let mut drops = IntervalsTree::new();
    drops.try_insert(1..=10).unwrap();
    drops.try_insert(3..=4).unwrap();
    drops.try_insert(5..=6).unwrap();
    assert_eq!(drops.to_vec(), vec![1..=10]);
}

#[test]
fn test_remove() {
    let mut drops = IntervalsTree::new();
    drops.insert(1);
    drops.remove(1);
    assert_eq!(drops.to_vec(), vec![]);

    let mut drops = IntervalsTree::new();
    drops.try_insert(1..=2).unwrap();
    drops.try_remove(1..=2).unwrap();
    assert_eq!(drops.to_vec(), vec![]);

    let mut drops = IntervalsTree::new();
    drops.try_insert(-1..=2).unwrap();
    drops.try_insert(4..=5).unwrap();
    drops.try_remove(-1..=2).unwrap();
    assert_eq!(drops.to_vec(), vec![4..=5]);

    let mut drops = IntervalsTree::new();
    drops.try_insert(-1..=2).unwrap();
    drops.try_insert(4..=5).unwrap();
    drops.try_remove(4..=5).unwrap();
    assert_eq!(drops.to_vec(), vec![-1..=2]);

    let mut drops = IntervalsTree::new();
    drops.try_insert(1..=2).unwrap();
    drops.try_insert(4..=5).unwrap();
    drops.try_remove(2..=4).unwrap();
    assert_eq!(drops.to_vec(), vec![1..=1, 5..=5]);

    let mut drops = IntervalsTree::new();
    drops.try_insert(-1..=2).unwrap();
    drops.try_insert(4..=5).unwrap();
    drops.try_remove(3..=4).unwrap();
    assert_eq!(drops.to_vec(), vec![-1..=2, 5..=5]);

    let mut drops = IntervalsTree::new();
    drops.try_insert(-1..=2).unwrap();
    drops.try_insert(4..=5).unwrap();
    drops.try_remove(-1..=5).unwrap();
    assert_eq!(drops.to_vec(), vec![]);

    let mut drops = IntervalsTree::new();
    drops.try_insert(1..=2).unwrap();
    drops.try_insert(4..=5).unwrap();
    drops.try_remove(2..=5).unwrap();
    assert_eq!(drops.to_vec(), vec![1..=1]);

    let mut drops = IntervalsTree::new();
    drops.try_insert(1..=2).unwrap();
    drops.try_insert(4..=5).unwrap();
    drops.try_remove(1..=4).unwrap();
    assert_eq!(drops.to_vec(), vec![5..=5]);

    let mut drops = IntervalsTree::new();
    drops.try_insert(1..=2).unwrap();
    drops.try_insert(4..=5).unwrap();
    drops.try_remove(1..=3).unwrap();
    assert_eq!(drops.to_vec(), vec![4..=5]);

    let mut drops = IntervalsTree::new();
    drops.try_insert(1..=10).unwrap();
    assert_eq!(drops.clone().to_vec(), vec![1..=10]);
    drops.remove(10..);
    assert_eq!(drops.clone().to_vec(), vec![1..=9]);
    drops.remove(2..);
    assert_eq!(drops.clone().to_vec(), vec![1..=1]);
    drops.try_insert(3..6).unwrap();
    assert_eq!(drops.clone().to_vec(), vec![1..=1, 3..=5]);
    drops.remove(..=3);
    assert_eq!(drops.clone().to_vec(), vec![4..=5]);
    drops.try_insert(1..=2).unwrap();
    assert_eq!(drops.clone().to_vec(), vec![1..=2, 4..=5]);
    drops.remove(..);
    assert_eq!(drops.clone().to_vec(), vec![]);
    drops.insert(..);
    assert_eq!(drops.clone().to_vec(), vec![i32::MIN..=i32::MAX]);
    drops.remove(..=9);
    assert_eq!(drops.clone().to_vec(), vec![10..=i32::MAX]);
    drops.remove(21..);
    assert_eq!(drops.clone().to_vec(), vec![10..=20]);
}

#[test]
fn test_interval_size() {
    assert_eq!(Interval::<u8>::try_from(11..111).unwrap().size(), Some(100),);
    assert_eq!(Interval::<u8>::try_from(..1).unwrap().size(), Some(1),);
    assert_eq!(Interval::<u8>::from(..=1).size(), Some(2));
    assert_eq!(Interval::<u8>::from(1..).size(), Some(255));
    assert_eq!(Interval::<u8>::from(0..).size(), None);
    assert_eq!(Interval::<u8>::from(..).size(), None);
    assert_eq!(Interval::<u8>::try_from(1..1).unwrap().size(), Some(0));

    assert_eq!(
        Interval::<u8>::try_from(11..111).unwrap().raw_size(),
        Some(100),
    );
    assert_eq!(Interval::<u8>::try_from(..1).unwrap().raw_size(), Some(1),);
    assert_eq!(Interval::<u8>::from(..=1).raw_size(), Some(2));
    assert_eq!(Interval::<u8>::from(1..).raw_size(), Some(255));
    assert_eq!(Interval::<u8>::from(0..).raw_size(), None);
    assert_eq!(Interval::<u8>::from(..).raw_size(), None);
    assert_eq!(Interval::<u8>::try_from(1..1).unwrap().raw_size(), Some(0));

    assert_eq!(Interval::<i8>::try_from(-1..99).unwrap().size(), Some(-28)); // corresponds to 100 numeration
    assert_eq!(Interval::<i8>::try_from(..1).unwrap().size(), Some(1)); // corresponds to 129 numeration
    assert_eq!(Interval::<i8>::from(..=1).size(), Some(2)); // corresponds to 130 numeration
    assert_eq!(Interval::<i8>::from(1..).size(), Some(-1)); // corresponds to 127 numeration
    assert_eq!(Interval::<i8>::from(0..).size(), Some(0)); // corresponds to 128 numeration
    assert_eq!(Interval::<i8>::from(..).size(), None); // corresponds to 256 numeration
    assert_eq!(Interval::<i8>::try_from(1..1).unwrap().size(), Some(-128)); // corresponds to 0 numeration

    assert_eq!(
        Interval::<i8>::try_from(-1..99).unwrap().raw_size(),
        Some(100)
    );
    assert_eq!(Interval::<i8>::try_from(..1).unwrap().raw_size(), Some(129));
    assert_eq!(Interval::<i8>::from(..=1).raw_size(), Some(130));
    assert_eq!(Interval::<i8>::from(1..).raw_size(), Some(127));
    assert_eq!(Interval::<i8>::from(0..).raw_size(), Some(128));
    assert_eq!(Interval::<i8>::from(..).raw_size(), None);
    assert_eq!(Interval::<i8>::try_from(1..1).unwrap().raw_size(), Some(0));
}

#[test]
fn test_interval_count_from() {
    assert_eq!(
        Interval::<u8>::count_from(0, 100).and_then(Interval::into_range_inclusive),
        Some(0..=99)
    );
    assert_eq!(
        Interval::<u8>::count_from(0, 255).and_then(Interval::into_range_inclusive),
        Some(0..=254)
    );
    assert_eq!(
        Interval::<u8>::count_from(0, None).and_then(Interval::into_range_inclusive),
        Some(0..=255)
    );
    assert_eq!(
        Interval::<u8>::count_from(1, 255).and_then(Interval::into_range_inclusive),
        Some(1..=255)
    );

    assert!(Interval::<u8>::count_from(1, 0).unwrap().is_empty());
    assert_eq!(Interval::<u8>::count_from(1, None), None);
    assert_eq!(Interval::<u8>::count_from(2, 255), None);
}

#[test]
fn test_try_voids() {
    let mut drops = IntervalsTree::new();
    drops.try_insert(1u32..=7).unwrap();
    drops.try_insert(19..=25).unwrap();
    assert_eq!(drops.clone().to_vec(), vec![1..=7, 19..=25]);
    assert_eq!(
        drops
            .try_voids(0..100)
            .unwrap()
            .map(RangeInclusive::from)
            .collect::<Vec<_>>(),
        vec![0..=0, 8..=18, 26..=99]
    );
    assert_eq!(
        drops
            .try_voids((0, None))
            .unwrap()
            .map(RangeInclusive::from)
            .collect::<Vec<_>>(),
        vec![0..=0, 8..=18, 26..=u32::MAX]
    );
    assert_eq!(
        drops
            .try_voids((None, None))
            .unwrap()
            .map(RangeInclusive::from)
            .collect::<Vec<_>>(),
        vec![]
    );
    assert_eq!(
        drops
            .try_voids(1..1)
            .unwrap()
            .map(RangeInclusive::from)
            .collect::<Vec<_>>(),
        vec![]
    );
    assert_eq!(
        drops
            .try_voids(0..=0)
            .unwrap()
            .map(RangeInclusive::from)
            .collect::<Vec<_>>(),
        vec![0..=0]
    );

    assert!(drops.try_voids(1..0).is_err());
}

#[test]
fn test_try_insert() {
    let mut drops = IntervalsTree::new();
    drops.try_insert(1u32..=2).unwrap();
    assert_eq!(drops.to_vec(), vec![1..=2]);
    drops.try_insert(4..=5).unwrap();
    assert_eq!(drops.to_vec(), vec![1..=2, 4..=5]);
    drops.try_insert(4..4).unwrap();
    assert_eq!(drops.to_vec(), vec![1..=2, 4..=5]);
    assert!(drops.try_insert(4..3).is_err());
    drops.try_insert(None..None).unwrap();
    assert_eq!(drops.to_vec(), vec![1..=2, 4..=5]);
    drops.try_insert((0, None)).unwrap();
    assert_eq!(drops.to_vec(), vec![0..=u32::MAX]);
}

#[test]
fn test_try_remove() {
    let mut drops = [1u32, 2, 5, 6, 7, 9, 10, 11]
        .into_iter()
        .collect::<IntervalsTree<_>>();
    assert_eq!(drops.to_vec(), vec![1..=2, 5..=7, 9..=11]);
    assert!(drops.try_remove(0..0).is_ok());
    assert_eq!(drops.to_vec(), vec![1..=2, 5..=7, 9..=11]);
    assert!(drops.try_remove(1..1).is_ok());
    assert_eq!(drops.to_vec(), vec![1..=2, 5..=7, 9..=11]);
    assert!(drops.try_remove(1..2).is_ok());
    assert_eq!(drops.to_vec(), vec![2..=2, 5..=7, 9..=11]);
    assert!(drops.try_remove(..7).is_ok());
    assert_eq!(drops.to_vec(), vec![7..=7, 9..=11]);
    assert!(drops.try_remove(None..None).is_ok());
    assert_eq!(drops.to_vec(), vec![7..=7, 9..=11]);
    assert!(drops.try_remove(1..0).is_err());
    assert_eq!(drops.to_vec(), vec![7..=7, 9..=11]);
    assert!(drops.try_remove((1, None)).is_ok());
    assert_eq!(drops.to_vec(), vec![]);
}

#[test]
fn test_contains() {
    let drops: IntervalsTree<u64> = [0, 100, 101, 102, 45678, 45679, 1, 2, 3]
        .into_iter()
        .collect();
    assert_eq!(drops.to_vec(), vec![0..=3, 100..=102, 45678..=45679]);
    assert!(drops.contains(0));
    assert!(drops.contains(1));
    assert!(drops.contains(2));
    assert!(drops.contains(3));
    assert!(!drops.contains(4));
    assert!(!drops.contains(99));
    assert!(drops.contains(100));
    assert!(drops.contains(101));
    assert!(drops.contains(102));
    assert!(!drops.contains(103));
    assert!(!drops.contains(45677));
    assert!(drops.contains(45678));
    assert!(drops.contains(45679));
    assert!(!drops.contains(45680));
    assert!(!drops.contains(141241));
    assert!(drops.try_contains(0..=3).unwrap());
    assert!(drops.try_contains(0..4).unwrap());
    assert!(!drops.try_contains(0..5).unwrap());
    assert!(drops.try_contains(1..1).unwrap());
    assert!(!drops.contains(..));
    assert!(drops.contains(..1));
}

#[test]
fn test_amount() {
    let drops: IntervalsTree<i32> = [-100, -99, 100, 101, 102, 1000].into_iter().collect();
    assert_eq!(drops.intervals_amount(), 3);
    assert_eq!(drops.points_amount(), Some(6));

    let drops: IntervalsTree<i32> = [..].into_iter().collect();
    assert_eq!(drops.intervals_amount(), 1);
    assert_eq!(drops.points_amount(), None);

    let drops: IntervalsTree<i32> = Default::default();
    assert_eq!(drops.intervals_amount(), 0);
    assert_eq!(drops.points_amount(), Some(0));
}

#[test]
fn test_start_end() {
    let drops: IntervalsTree<u64> = [0u64, 100, 101, 102, 45678, 45679, 1, 2, 3]
        .into_iter()
        .collect();
    assert_eq!(drops.to_vec(), vec![0..=3, 100..=102, 45678..=45679]);
    assert_eq!(drops.start(), Some(0));
    assert_eq!(drops.end(), Some(45679));
}

#[test]
fn test_and_not_iter() {
    let drops: IntervalsTree<u64> = [0, 1, 2, 3, 4, 8, 9, 100, 101, 102].into_iter().collect();
    let drops1: IntervalsTree<u64> = [3, 4, 7, 8, 9, 10, 45, 46, 100, 102].into_iter().collect();
    let v: Vec<RangeInclusive<u64>> = drops.and_not_iter(&drops1).map(Into::into).collect();
    assert_eq!(v, vec![0..=2, 101..=101]);

    let drops1: IntervalsTree<u64> = [..].into_iter().collect();
    let v: Vec<RangeInclusive<u64>> = drops.and_not_iter(&drops1).map(Into::into).collect();
    assert_eq!(v, vec![]);

    let drops1: IntervalsTree<u64> = [..=100].into_iter().collect();
    let v: Vec<RangeInclusive<u64>> = drops.and_not_iter(&drops1).map(Into::into).collect();
    assert_eq!(v, vec![101..=102]);

    let drops1: IntervalsTree<u64> = [101..].into_iter().collect();
    let v: Vec<RangeInclusive<u64>> = drops.and_not_iter(&drops1).map(Into::into).collect();
    assert_eq!(v, vec![0..=4, 8..=9, 100..=100]);

    let drops1: IntervalsTree<u64> = [6, 10, 110].into_iter().collect();
    let v: Vec<RangeInclusive<u64>> = drops.and_not_iter(&drops1).map(Into::into).collect();
    assert_eq!(v, vec![0..=4, 8..=9, 100..=102]);
}

mod stress_tests {
    use crate::{
        mock::{self, TreeAction},
        BoundValue, Interval, Numerated,
    };
    use alloc::vec::Vec;
    use rand::{rngs::StdRng, Rng, SeedableRng};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, derive_more::Into)]
    struct Number(i32);

    impl Numerated for Number {
        type N = u32;
        type B = BoundValue<Self>;

        fn raw_add_if_lt(self, num: Self::N, other: Self) -> Option<Self> {
            let num = <i32>::try_from(num).unwrap();
            self.0
                .checked_add(num)
                .filter(|&res| res <= other.0)
                .map(Self)
        }
        fn raw_sub_if_gt(self, num: Self::N, other: Self) -> Option<Self> {
            let num = <i32>::try_from(num).unwrap();
            self.0
                .checked_sub(num)
                .filter(|&res| res >= other.0)
                .map(Self)
        }
        fn sub(self, other: Self) -> Option<Self::N> {
            self.0.checked_sub(other.0).map(|res| res as u32)
        }
    }

    fn rand_interval(rng: &mut StdRng, max: i32, min: i32, max_len: u32) -> Interval<Number> {
        let max_len = <i32>::try_from(max_len).unwrap();
        let end = rng.gen_range(min..=max);
        let start = rng.gen_range(end.saturating_sub(max_len)..=end);
        (Number(start)..=Number(end)).try_into().unwrap()
    }

    fn stress_test(
        rng: &mut StdRng,
        max: i32,
        min: i32,
        actions_amount: usize,
        interval_max_len: u32,
    ) {
        let drops_diapason_max_len = interval_max_len * 3;

        let actions = (0..actions_amount)
            .map(|_| match rng.gen_range(0..3) {
                0 => TreeAction::Insert(rand_interval(rng, max, min, interval_max_len)),
                1 => TreeAction::Remove(rand_interval(rng, max, min, interval_max_len)),
                2 => TreeAction::Voids(rand_interval(rng, max, min, drops_diapason_max_len)),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();

        let initial = (min..=max)
            .filter(|_| rng.gen_range(0..10) == 0)
            .map(Number)
            .collect();

        mock::test_tree(initial, actions);
    }

    #[test]
    fn stress_simple() {
        env_logger::init();
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..100_000 {
            stress_test(&mut rng, 100, -100, 10, 20);
        }
    }

    #[ignore = "takes too long"]
    #[test]
    fn stress_hard() {
        env_logger::init();
        let mut rng = StdRng::seed_from_u64(43);
        for _ in 0..1_000_000 {
            stress_test(&mut rng, 10_000, -1_000, 100, 100);
        }
    }
}
