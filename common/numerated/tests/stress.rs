use num_traits::bounds::{LowerBounded, UpperBounded};
use numerated::{BoundValue, Interval, IntervalsTree, Numerated};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, derive_more::Into)]
struct Number<const MAX: u32>(u32);

impl<const MAX: u32> Numerated for Number<MAX> {
    type N = u32;
    type B = BoundValue<Self>;

    fn raw_add_if_lt(self, num: u32, other: Self) -> Option<Self> {
        if num == 0 || (self < other && other.0 - self.0 >= num) {
            Some(Self(self.0 + num))
        } else {
            None
        }
    }
    fn raw_sub_if_gt(self, num: Self::N, other: Self) -> Option<Self> {
        if num == 0 || (self > other && self.0 - other.0 >= num) {
            Some(Self(self.0 - num))
        } else {
            None
        }
    }
    fn sub(self, other: Self) -> Option<Self::N> {
        self.0.checked_sub(other.0)
    }
}

impl<const MAX: u32> LowerBounded for Number<MAX> {
    fn min_value() -> Self {
        Self(0)
    }
}

impl<const MAX: u32> UpperBounded for Number<MAX> {
    fn max_value() -> Self {
        Self(MAX)
    }
}

// TODO: generate interval from different type of ranges
impl<const MAX: u32> Number<MAX> {
    fn rand_interval(rng: &mut StdRng, max_len: u32) -> Interval<Number<MAX>> {
        let end = rng.gen_range(0..=MAX);
        let start = rng.gen_range(end.saturating_sub(max_len)..=end);
        (Number(start)..Number(end)).try_into().unwrap()
    }
}

fn stress_test<const MAX: u32>(
    iterations: usize,
    actions_per_test: usize,
    max_interval_length: u32,
    drops_check_iterations: usize,
) {
    let drops_diapason_max_len = max_interval_length * 3;

    let mut rng = StdRng::seed_from_u64(42);

    for i in 0..iterations {
        log::debug!("Iteration: {}", i);
        let mut drops = IntervalsTree::new();
        let mut expected = BTreeSet::new();
        for _ in 0..actions_per_test {
            let interval = Number::<MAX>::rand_interval(&mut rng, max_interval_length);
            if rng.gen_bool(0.5) {
                log::trace!("remove {interval:?} from {drops:?}");
                drops.remove(interval);
                interval.for_each(|i| {
                    expected.remove(&i);
                });
            } else {
                log::trace!("insert {interval:?} in {drops:?}");
                drops.insert(interval);
                expected.extend(interval);
            }
        }
        let drops_set: BTreeSet<_> = drops.points_iter().collect();
        assert_eq!(drops_set, expected);
        let expected_drops: IntervalsTree<_> = expected.iter().collect();
        assert_eq!(drops, expected_drops);

        let complement = drops.complement();
        log::trace!("drops = {:?}", drops);
        log::trace!("complement = {:?}", complement);
        for j in 0..drops_check_iterations {
            log::trace!("j = {j}");
            let mut complement = complement.clone();
            let interval = Number::rand_interval(&mut rng, drops_diapason_max_len);
            complement.crop(interval);
            log::trace!("interval {:?}", interval);
            assert_eq!(complement, drops.voids(interval).collect());
        }
    }
}

#[test]
fn stress_simple() {
    env_logger::init();
    stress_test::<100>(100_000, 10, 10, 100);
}

#[ignore = "takes too long"]
#[test]
fn stress_hard() {
    env_logger::init();
    stress_test::<10_000>(1_000_000, 100, 1000, 1000);
}
