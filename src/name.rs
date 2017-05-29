use rand::{Rand, Rng};
use std::cmp::Ordering;
use std::fmt::{self, Binary, Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::mem;
use std::u64;

/// Node names are u64s.
#[derive(PartialOrd, Ord, PartialEq, Eq, Copy, Clone, Hash, Default)]
pub struct Name(pub u64);

#[allow(dead_code)]
impl Name {
    /// Returns the length of the common prefix with the `other` name; e. g.
    /// the when `other = 11110000` and `self = 11111111` this is 4.
    pub fn common_prefix(&self, other: Self) -> usize {
        (self.0 ^ other.0).leading_zeros() as usize
    }

    /// Compares the distance of the arguments to `self`. Returns `Less` if `lhs` is closer,
    /// `Greater` if `rhs` is closer, and `Equal` if `lhs == rhs`. (The XOR distance can only be
    /// equal if the arguments ar equal.)
    pub fn cmp_distance(&self, lhs: Self, rhs: Self) -> Ordering {
        Ord::cmp(&(lhs.0 ^ self.0), &(rhs.0 ^ self.0))
    }

    /// Returns `true` if the `i`-th bit is `1`.
    pub fn bit(&self, i: usize) -> bool {
        let pow_i = 1 << (mem::size_of::<Self>() * 8 - 1 - i); // 1 on bit i.
        self.0 & pow_i != 0
    }

    /// Returns a copy of `self`, with the `index`-th bit set to `bit`.
    ///
    /// If `index` exceeds the number of bits in `self`, an unmodified copy of `self` is returned.
    pub fn with_bit(mut self, i: usize, bit: bool) -> Self {
        if i >= mem::size_of::<Self>() * 8 {
            return self;
        }
        let pow_i = 1 << (mem::size_of::<Self>() * 8 - 1 - i); // 1 on bit i.
        if bit {
            self.0 |= pow_i;
        } else {
            self.0 &= !pow_i;
        }
        self
    }

    /// Returns a copy of `self`, with the `index`-th bit flipped.
    ///
    /// If `index` exceeds the number of bits in `self`, an unmodified copy of `self` is returned.
    pub fn with_flipped_bit(mut self, i: usize) -> Self {
        if i >= mem::size_of::<Self>() * 8 {
            return self;
        }
        let pow_i = 1 << (mem::size_of::<Self>() * 8 - 1 - i); // 1 on bit i.
        self.0 ^= pow_i;
        self
    }

    /// Returns a copy of self with first `n` bits preserved, and remaining bits
    /// set to 0 (val == false) or 1 (val == true).
    pub fn set_remaining(mut self, n: usize, val: bool) -> Self {
        let bits = mem::size_of::<u64>() * 8;
        if n < bits {
            let mask = !0 >> n;
            if val { self.0 |= mask } else { self.0 &= !mask }
        }
        self
    }
}

/// Prints full 64 character binary representation of `Name`, including leading zeros.
impl Binary for Name {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter, "{1:00$b}", mem::size_of::<Self>() * 8, self.0)
    }
}

/// Prints abbreviated hex representation of `Name`.   This is the first six characters of the full
/// hex representation including leading zeros.
impl Debug for Name {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        (self as &Display).fmt(formatter)
    }
}

/// Prints abbreviated hex representation of `Name`.   This is the first six characters of the full
/// hex representation including leading zeros.
impl Display for Name {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        let mut hex = format!("{1:00$x}", mem::size_of::<Self>() * 2, self.0);
        hex.truncate(6);
        write!(formatter, "{}..", hex)
    }
}

impl Rand for Name {
    fn rand<R: Rng>(rng: &mut R) -> Name {
        Name(rng.gen())
    }
}

// A group prefix, i.e. a sequence of bits specifying the part of the network's name space
// consisting of all names that start with this sequence.
#[derive(Clone, Copy, Default, Eq, Ord)]
pub struct Prefix {
    bit_count: usize,
    name: Name,
}

impl Prefix {
    /// Creates a new `Prefix` with the first `bit_count` bits of `name`.
    /// Insignificant bits are all set to 0.
    pub fn new(bit_count: usize, name: Name) -> Prefix {
        Prefix {
            bit_count: bit_count,
            name: name.set_remaining(bit_count, false),
        }
    }

    /// The empty prefix, ().
    pub fn empty() -> Prefix {
        Prefix::new(0, Name(0))
    }

    /// Create a `Prefix` using the given byte as the highest order byte of the prefix.
    pub fn short(bit_count: usize, name: u8) -> Prefix {
        let long_name = (name as u64) << (64 - 8);
        Prefix::new(bit_count, Name(long_name))
    }

    /// Compute the length of the common prefix of this prefix and the given name.
    pub fn common_prefix(&self, name: Name) -> usize {
        self.name.common_prefix(name)
    }

    /// Returns `self` with an appended bit: `0` if `bit` is `false`, and `1` if `bit` is `true`.
    pub fn pushed(mut self, bit: bool) -> Prefix {
        self.name = self.name.with_bit(self.bit_count, bit);
        self.bit_count += 1;
        self
    }

    /// Returns a prefix copying the first `bitcount() - 1` bits from `self`,
    /// or `self` if it is already empty.
    pub fn popped(mut self) -> Prefix {
        if self.bit_count > 0 {
            self.bit_count -= 1;
            // unused bits should be zero:
            self.name = self.name.with_bit(self.bit_count, false);
        }
        self
    }

    /// Returns the number of bits in the prefix.
    pub fn bit_count(&self) -> usize {
        self.bit_count
    }

    /// Returns `true` if `self` is a prefix of `other` or vice versa.
    pub fn is_compatible(&self, other: &Prefix) -> bool {
        let i = self.name.common_prefix(other.name);
        i >= self.bit_count || i >= other.bit_count
    }

    /// Returns `true` if `self` is a prefix of `other`.
    pub fn is_prefix_of(&self, other: &Prefix) -> bool {
        let i = self.name.common_prefix(other.name);
        i >= self.bit_count
    }

    /// Returns `true` if the `other` prefix differs in exactly one bit from this one.
    pub fn is_neighbour(&self, other: &Prefix) -> bool {
        let i = self.name.common_prefix(other.name);
        if i >= self.bit_count() || i >= other.bit_count() {
            false
        } else {
            let j = self.name.with_flipped_bit(i).common_prefix(other.name);
            j >= self.bit_count() || j >= other.bit_count()
        }
    }

    /// Returns `true` if this is a prefix of the given `name`.
    pub fn matches(&self, name: Name) -> bool {
        self.name.common_prefix(name) >= self.bit_count
    }

    /// Returns whether the namespace defined by `self` is covered by prefixes in the `prefixes`
    /// set
    pub fn is_covered_by<'a, U>(&self, prefixes: U) -> bool
        where U: IntoIterator<Item = &'a Prefix> + Clone
    {
        let max_prefix_len = prefixes
            .clone()
            .into_iter()
            .map(|x| x.bit_count())
            .max()
            .unwrap_or(0);
        self.is_covered_by_impl(prefixes, max_prefix_len)
    }

    fn is_covered_by_impl<'a, U>(&self, prefixes: U, max_prefix_len: usize) -> bool
        where U: IntoIterator<Item = &'a Prefix> + Clone
    {
        prefixes
            .clone()
            .into_iter()
            .any(|x| x.is_compatible(self) && x.bit_count() <= self.bit_count()) ||
        (self.bit_count() <= max_prefix_len &&
         self.pushed(false)
             .is_covered_by_impl(prefixes.clone(), max_prefix_len) &&
         self.pushed(true)
             .is_covered_by_impl(prefixes, max_prefix_len))
    }

    /// Returns the given `name` with first bits replaced by `self`
    pub fn substituted_in(&self, mut name: Name) -> Name {
        // TODO: is there a more efficient way of doing that?
        for i in 0..self.bit_count() {
            name = name.with_bit(i, self.name.bit(i));
        }
        name
    }

    /// Returns the prefix that is the sibling of `self` (if one exists)
    pub fn sibling(&self) -> Option<Prefix> {
        if self.bit_count > 0 {
            Some(Prefix {
                     name: self.name.with_flipped_bit(self.bit_count - 1),
                     bit_count: self.bit_count,
                 })
        } else {
            None
        }
    }
}

impl PartialEq<Prefix> for Prefix {
    fn eq(&self, other: &Self) -> bool {
        self.is_compatible(other) && self.bit_count == other.bit_count
    }
}

impl PartialOrd<Prefix> for Prefix {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self == other {
            Some(Ordering::Equal)
        } else if self.is_compatible(other) {
            None
        } else {
            Some(self.name.cmp(&other.name))
        }
    }
}

impl Hash for Prefix {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for i in 0..self.bit_count {
            self.name.bit(i).hash(state);
        }
    }
}

impl Binary for Prefix {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        let mut binary = format!("{:b}", self.name);
        binary.truncate(self.bit_count);
        write!(formatter, "Prefix({})", binary)
    }
}

impl Debug for Prefix {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        Binary::fmt(self, formatter)
    }
}
