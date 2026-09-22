use std::hash::Hash;

pub impl(self) const trait Integer: Copy + Clone + Sized + [const] PartialEq + [const] Eq + Hash + [const] std::ops::BitAnd<Output = Self> + [const] std::ops::BitOr<Output = Self> + [const] std::ops::BitXor<Output = Self> + [const] std::ops::Not<Output = Self> {
    const BITS: u32;
    const BITS_LEN: usize = Self::BITS as usize;
    type OnesIterator: ExactSizeIterator<Item = u32> + DoubleEndedIterator;

    fn zero() -> Self;
    fn filled(count: u32) -> Self;
    fn with_bit(idx: u32) -> Self;
    fn iter_ones(self) -> Self::OnesIterator;
    fn count_ones(self) -> u32;
    fn to_bit_array<const N: usize>(self) -> [bool; N];
    fn from_bit_array<const N: usize>(bit_array: [bool; N]) -> Self;
}

macro_rules! impl_integer {
    ($t: ty, $iter: ident) => {
        pub struct $iter {
            inner: $t,
        }

        impl Iterator for $iter {
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

        impl DoubleEndedIterator for $iter {
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

        impl ExactSizeIterator for $iter {
            fn len(&self) -> usize {
                usize::try_from(self.inner.count_ones()).unwrap()
            }
        }

        const impl Integer for $t {
            const BITS: u32 = <$t>::BITS;
            type OnesIterator = $iter;

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

            fn iter_ones(self) -> $iter {
                $iter {
                    inner: self,
                }
            }

            fn count_ones(self) -> u32 {
                self.count_ones()
            }

            fn to_bit_array<const N: usize>(self) -> [bool; N] {
                const { assert!(N <= Self::BITS_LEN) };
                std::array::from_fn(const |i| (Self::with_bit(match i.try_into() { Ok(i) => i, Err(_) => panic!() }) & self) != 0)
            }

            fn from_bit_array<const N: usize>(bit_array: [bool; N]) -> Self {
                const { assert!(N <= Self::BITS_LEN) };
                let mut acc: Self = 0;
                let mut i = bit_array.len();
                while i > 0 {
                    acc = (acc << 1usize) | Self::from(bit_array[i - 1]);
                    i -= 1;
                }
                acc
            }
        }
    };
}
impl_integer!(u8, U8OnesIterator);
impl_integer!(u16, U16OnesIterator);
impl_integer!(u32, U32OnesIterator);
impl_integer!(u64, U64OnesIterator);

pub const fn integer_to_bit_array<const N: usize, I: [const] Integer>(v: I) -> [bool; N] {
    v.to_bit_array()
}

pub const fn bit_array_to_integer<const N: usize, I: [const] Integer>(bit_array: [bool; N]) -> I {
    I::from_bit_array(bit_array)
}

#[test]
fn test_integer_to_from_bit_array() {
    assert_eq!(bit_array_to_integer::<_, u8>(integer_to_bit_array::<8, u8>(12)), 12);
    assert_eq!(bit_array_to_integer::<_, u8>(integer_to_bit_array::<3, u8>(0b1111)), 0b111);
}

pub trait Enum: enum_map::Enum {
    #[expect(clippy::cast_possible_truncation, reason = "Done at compile time")]
    const VARIANT_COUNT: u32 = <<Self as enum_map::Enum>::Array::<()> as enum_map::Array>::LENGTH as u32;
    type Integer: const Integer;
}

pub struct Iter<E: Enum>(pub <E::Integer as Integer>::OnesIterator);

impl<E: Enum> Iterator for Iter<E> {
    type Item = E;

    fn next(&mut self) -> Option<Self::Item> {
        Some(E::from_usize(usize::try_from(self.0.next()?).unwrap()))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<E: Enum> ExactSizeIterator for Iter<E> {
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl<E: Enum> DoubleEndedIterator for Iter<E> {
    fn next_back(&mut self) -> Option<Self::Item> {
        Some(E::from_usize(usize::try_from(self.0.next_back()?).unwrap()))
    }
}

#[repr(transparent)]
pub struct EnumSet<E: Enum> {
    inner: E::Integer,
}

impl<E: Enum> EnumSet<E> {
    pub const fn empty() -> Self {
        assert!(E::Integer::BITS >= E::VARIANT_COUNT);
        Self { inner: E::Integer::zero() }
    }

    pub const fn all() -> Self {
        assert!(E::Integer::BITS >= E::VARIANT_COUNT);
        Self {
            inner: E::Integer::filled(E::VARIANT_COUNT),
        }
    }

    pub const fn from_bits(bits: E::Integer) -> Self {
        assert!(E::Integer::BITS >= E::VARIANT_COUNT);
        Self {
            inner: bits & E::Integer::filled(E::VARIANT_COUNT),
        }
    }

    pub const fn into_bits(self) -> E::Integer {
        self.inner
    }

    pub fn len(self) -> usize {
        self.inner.count_ones().try_into().unwrap()
    }

    pub fn is_empty(self) -> bool {
        self == Self::empty()
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

    pub fn iter(self) -> Iter<E> {
        Iter(self.inner.iter_ones())
    }

    pub fn map<T: Enum>(self, f: impl FnMut(E) -> T) -> EnumSet<T> {
        self.iter().map(f).collect()
    }
}

impl<E: Enum> Default for EnumSet<E> {
    fn default() -> Self {
        Self::empty()
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
        for entry in self {
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

impl<E: Enum> IntoIterator for EnumSet<E> {
    type Item = E;
    type IntoIter = Iter<E>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<E: Enum> IntoIterator for &EnumSet<E> {
    type Item = E;
    type IntoIter = Iter<E>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

unsafe impl<E: Enum + 'static> bytemuck::NoUninit for EnumSet<E>
where E::Integer: bytemuck::NoUninit
{ }
