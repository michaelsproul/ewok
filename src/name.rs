use std::cmp::Ordering;
use std::mem;
use std::hash::{Hash, Hasher};
use std::fmt::{self, Formatter, Binary, Debug};
use std::u64;

/// Node names are u64s.
pub type Name = u64;

/// Name type (Xorable in routing library).
trait NameT: Ord {
    /// Returns the length of the common prefix with the `other` name; e. g.
    /// the when `other = 11110000` and `self = 11111111` this is 4.
    fn common_prefix(&self, other: Self) -> usize;

    /// Compares the distance of the arguments to `self`. Returns `Less` if `lhs` is closer,
    /// `Greater` if `rhs` is closer, and `Equal` if `lhs == rhs`. (The XOR distance can only be
    /// equal if the arguments ar equal.)
    fn cmp_distance(&self, lhs: Self, rhs: Self) -> Ordering;

    /// Returns `true` if the `i`-th bit is `1`.
    fn bit(&self, i: usize) -> bool;

    /// Returns a copy of `self`, with the `index`-th bit set to `bit`.
    ///
    /// If `index` exceeds the number of bits in `self`, an unmodified copy of `self` is returned.
    fn with_bit(self, i: usize, bit: bool) -> Self;

    /// Returns a binary format string, with leading zero bits included.
    fn binary(&self) -> String;

    /// Returns a copy of self with first `n` bits preserved, and remaining bits
    /// set to 0 (val == false) or 1 (val == true).
    fn set_remaining(self, n: usize, val: bool) -> Self;
}

impl NameT for u64 {
    fn common_prefix(&self, other: Self) -> usize {
        (self ^ other).leading_zeros() as usize
    }

    fn cmp_distance(&self, lhs: Self, rhs: Self) -> Ordering {
        Ord::cmp(&(lhs ^ self), &(rhs ^ self))
    }

    fn bit(&self, i: usize) -> bool {
        let pow_i = 1 << (mem::size_of::<Self>() * 8 - 1 - i); // 1 on bit i.
        self & pow_i != 0
    }

    fn with_bit(mut self, i: usize, bit: bool) -> Self {
        if i >= mem::size_of::<Self>() * 8 {
            return self;
        }
        let pow_i = 1 << (mem::size_of::<Self>() * 8 - 1 - i); // 1 on bit i.
        if bit {
            self |= pow_i;
        } else {
            self &= !pow_i;
        }
        self
    }

    fn binary(&self) -> String {
        format!("{1:00$b}", mem::size_of::<Self>() * 8, self)
    }

    fn set_remaining(self, n: usize, val: bool) -> Self {
        let bits = mem::size_of::<u64>() * 8;
        if n >= bits {
            self
        } else {
            let mask = !0 >> n;
            if val { self | mask } else { self & !mask }
        }
    }
}

// A group prefix, i.e. a sequence of bits specifying the part of the network's name space
// consisting of all names that start with this sequence.
#[derive(Clone, Copy, Default, Eq, Ord)]
pub struct Prefix {
    bit_count: usize,
    name: u64,
}

impl Prefix {
    /// Creates a new `Prefix` with the first `bit_count` bits of `name`.
    /// Insignificant bits are all set to 0.
    pub fn new(bit_count: usize, name: u64) -> Prefix {
        Prefix {
            bit_count: bit_count,
            name: name.set_remaining(bit_count, false),
        }
    }

    /// Compute the length of the common prefix of this prefix and the given name.
    pub fn common_prefix(&self, name: u64) -> usize {
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
    pub fn is_compatible(&self, other: Prefix) -> bool {
        let i = self.name.common_prefix(other.name);
        i >= self.bit_count || i >= other.bit_count
    }

    /// Returns `true` if `self` is a prefix of `other`.
    pub fn is_prefix_of(&self, other: Prefix) -> bool {
        let i = self.name.common_prefix(other.name);
        i >= self.bit_count
    }

    /// Returns `true` if this is a prefix of the given `name`.
    pub fn matches(&self, name: u64) -> bool {
        self.name.common_prefix(name) >= self.bit_count
    }
}

impl PartialEq<Prefix> for Prefix {
    fn eq(&self, other: &Self) -> bool {
        self.is_compatible(*other) && self.bit_count == other.bit_count
    }
}

impl PartialOrd<Prefix> for Prefix {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self == other {
            Some(Ordering::Equal)
        } else if self.is_compatible(*other) {
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
        let mut binary = self.name.binary();
        binary.truncate(self.bit_count);
        write!(formatter, "Prefix({})", binary)
    }
}

impl Debug for Prefix {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        Binary::fmt(self, formatter)
    }
}
