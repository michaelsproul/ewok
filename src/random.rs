use rand::{self, thread_rng, XorShiftRng, Rand, Rng, SeedableRng};
use std::cell::RefCell;
use std::env;

thread_local! {
    static WEAK_RNG: RefCell<XorShiftRng> = RefCell::new({
        let seed = match env::var("EWOK_SEED") {
            Ok(value) => {
                let nums: Vec<u32> = value.split(|c| c == '[' || c == ']' || c == ' ' || c == ',')
                                          .filter_map(|s| s.parse().ok())
                                          .collect();
                assert_eq!(nums.len(), 4, "EWOK_SEED {} isn't in the form '[1, 2, 3, 4]'.", value);
                [nums[0], nums[1], nums[2], nums[3]]
            }
            Err(_) => {
                let mut rng = thread_rng();
                [rng.next_u32().wrapping_add(rng.next_u32()),
                 rng.next_u32().wrapping_add(rng.next_u32()),
                 rng.next_u32().wrapping_add(rng.next_u32()),
                 rng.next_u32().wrapping_add(rng.next_u32())]
            }
        };
        info!("Seed: {:?}", seed);
        XorShiftRng::from_seed(seed)
    });
}

/// Random value from the thread-local weak RNG.
pub fn random<T: Rand>() -> T {
    WEAK_RNG.with(|rng| rng.borrow_mut().gen())
}

/// Sample values from an iterator.
pub fn sample<T, I>(iterable: I, amount: usize) -> Vec<T>
    where I: IntoIterator<Item = T>
{
    WEAK_RNG.with(|rng| rand::sample(&mut *rng.borrow_mut(), iterable, amount))
}

/// Sample a single value from an iterator.
pub fn sample_single<T, I>(iterable: I) -> Option<T>
    where I: IntoIterator<Item = T>
{
    sample(iterable, 1).pop()
}

/// Return true with probability p.
pub fn do_with_probability(p: f64) -> bool {
    random::<f64>() <= p
}
