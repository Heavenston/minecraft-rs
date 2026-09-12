#![expect(dead_code, reason = "Utils with functions maybe not used")]

use std::hash::Hash;

pub impl(self) trait Integer: Copy + Clone + Sized + PartialEq + Eq + Hash + std::ops::BitAnd<Output = Self> + std::ops::BitOr<Output = Self> + std::ops::BitXor<Output = Self> + std::ops::Not<Output = Self> {
    const BITS: u32;

    fn zero() -> Self;
    fn filled(count: u32) -> Self;
    fn with_bit(idx: u32) -> Self;
    fn iter_ones(self) -> impl ExactSizeIterator<Item = u32> + DoubleEndedIterator;
    fn count_ones(self) -> u32;
}

macro_rules! impl_integer {
    ($t: ty) => {
        impl Integer for $t {
            const BITS: u32 = <$t>::BITS;

            fn zero() -> Self {
                0
            }

            fn filled(count: u32) -> Self {
                assert!(count <= Self::BITS);
                (!Self::zero()).unbounded_shr(Self::BITS - count)
            }

            fn with_bit(idx: u32) -> Self {
                assert!(idx < Self::BITS);
                1 << idx
            }

            fn iter_ones(self) -> impl ExactSizeIterator<Item = u32> + DoubleEndedIterator {
                struct Iter {
                    inner: $t,
                }

                impl Iterator for Iter {
                    type Item = u32;

                    fn next(&mut self) -> Option<u32> {
                        let next = self.inner.trailing_zeros();
                        if next == <$t>::BITS {
                            None
                        }
                        else {
                            self.inner &= (!<$t>::zero()).unbounded_shl(next+1);
                            Some(next)
                        }
                    }

                    fn size_hint(&self) -> (usize, Option<usize>) {
                        let len = self.len();
                        (len, Some(len))
                    }
                }

                impl DoubleEndedIterator for Iter {
                    fn next_back(&mut self) -> Option<u32> {
                        let next_leading = self.inner.leading_zeros();
                        if next_leading == <$t>::BITS {
                            None
                        }
                        else {
                            self.inner &= (!<$t>::MIN).unbounded_shr(next_leading+1);
                            Some(<$t>::BITS - next_leading - 1)
                        }
                    }
                }

                impl ExactSizeIterator for Iter {
                    fn len(&self) -> usize {
                        usize::try_from(self.inner.count_ones()).unwrap()
                    }
                }

                Iter {
                    inner: self,
                }
            }

            fn count_ones(self) -> u32 {
                self.count_ones()
            }
        }
    };
}
impl_integer!(u8);
impl_integer!(u16);
impl_integer!(u32);
impl_integer!(u64);

pub trait Enum: enum_map::Enum {
    #[expect(clippy::cast_possible_truncation, reason = "Done at compile time")]
    const VARIANT_COUNT: u32 = <<Self as enum_map::Enum>::Array::<()> as enum_map::Array>::LENGTH as u32;
    type Integer: Integer;
}

pub struct EnumSet<E: Enum> {
    inner: E::Integer,
}

impl<E: Enum> EnumSet<E> {
    pub fn empty() -> Self {
        assert!(E::Integer::BITS >= E::VARIANT_COUNT);
        Self { inner: E::Integer::zero() }
    }

    pub fn all() -> Self {
        assert!(E::Integer::BITS >= E::VARIANT_COUNT);
        Self {
            inner: E::Integer::filled(E::VARIANT_COUNT),
        }
    }

    pub fn len(self) -> usize {
        self.inner.count_ones().try_into().unwrap()
    }

    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    pub fn is_all(self) -> bool {
        self == Self::all()
    }

    pub fn inverse(self) -> Self {
        Self {
            inner: self.inner ^ E::Integer::filled(E::VARIANT_COUNT),
        }
    }

    pub fn intersection(self, rhs: Self) -> Self {
        Self {
            inner: self.inner & rhs.inner,
        }
    }

    pub fn union(self, rhs: Self) -> Self {
        Self {
            inner: self.inner | rhs.inner,
        }
    }

    pub fn difference(self, rhs: Self) -> Self {
        Self {
            inner: self.inner & !rhs.inner,
        }
    }

    /// Returns true if `rhs` contains all elements of `self`.
    pub fn is_subset(self, rhs: Self) -> bool {
        rhs.intersection(self) == self
    }

    /// Returns true if `self` contains all elements of `rhs`.
    pub fn is_superset(self, rhs: Self) -> bool {
        self.intersection(rhs) == rhs
    }

    pub fn with(self, value: E) -> Self {
        Self {
            inner: self.inner | E::Integer::with_bit(u32::try_from(E::into_usize(value)).unwrap()),
        }
    }

    pub fn insert(&mut self, value: E) {
        *self = self.with(value);
    }

    pub fn without(self, value: E) -> Self {
        self.difference(Self::empty().with(value))
    }

    pub fn remove(&mut self, value: E) {
        *self = self.with(value);
    }

    pub fn contains(self, value: E) -> bool {
        !self.intersection(Self::empty().with(value)).is_empty()
    }

    pub fn clear(&mut self) {
        *self = Self::empty();
    }

    pub fn iter(self) -> impl ExactSizeIterator<Item = E> + DoubleEndedIterator {
        self.inner.iter_ones().map(|b| E::from_usize(usize::try_from(b).unwrap()))
    }
}

impl<E: Enum> Default for EnumSet<E> {
    fn default() -> Self {
        Self { inner: E::Integer::zero() }
    }
}

impl<E: Enum> Clone for EnumSet<E> {
    fn clone(&self) -> Self { *self }
}
impl<E: Enum> Copy for EnumSet<E> { }

impl<E: Enum> PartialEq for EnumSet<E> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
impl<E: Enum> Eq for EnumSet<E> { }

impl<E: Enum> Hash for EnumSet<E> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}

impl<E: Enum + std::fmt::Debug> std::fmt::Debug for EnumSet<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_set();
        for entry in self.iter() {
            debug.entry(&entry);
        }
        debug.finish()
    }
}

impl<E: Enum> std::ops::BitOr for EnumSet<E> {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl<E: Enum> std::ops::BitOr<E> for EnumSet<E> {
    type Output = Self;
    fn bitor(self, rhs: E) -> Self::Output {
        self.with(rhs)
    }
}

impl<E: Enum> std::ops::BitAnd for EnumSet<E> {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self::Output {
        self.intersection(rhs)
    }
}

impl<E: Enum> Extend<E> for EnumSet<E> {
    fn extend<T: IntoIterator<Item = E>>(&mut self, iter: T) {
        for value in iter {
            self.insert(value);
        }
    }
}

impl<E: Enum> FromIterator<E> for EnumSet<E> {
    fn from_iter<T: IntoIterator<Item = E>>(iter: T) -> Self {
        let mut set = Self::empty();
        set.extend(iter);
        set
    }
}
