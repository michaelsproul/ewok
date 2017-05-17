use rand::{self, weak_rng, XorShiftRng, Rand, Rng};
use std::cell::RefCell;

thread_local! {
    static WEAK_RNG: RefCell<XorShiftRng> = RefCell::new(weak_rng());
}

/// Random value from the thread-local weak RNG.
pub fn random<T: Rand>() -> T {
    WEAK_RNG.with(|rng| {
        rng.borrow_mut().gen()
    })
}

/// Sample values from an iterator.
pub fn sample<T, I>(iterable: I, amount: usize) -> Vec<T>
    where I: IntoIterator<Item=T>
{
    WEAK_RNG.with(|rng| {
        rand::sample(&mut *rng.borrow_mut(), iterable, amount)
    })
}
