use std::{borrow::Borrow, marker::PhantomData, ops::{Deref, DerefMut, Index, IndexMut}};
use ref_cast::{ref_cast_custom, RefCastCustom};

pub trait MapIndex {
    fn from_usize(idx: usize) -> Self;
    fn as_usize(&self) -> usize;
}

#[derive(RefCastCustom)]
#[repr(transparent)]
pub struct IndexSlice<T, I> {
    index: PhantomData<fn(I) -> I>,
    values: [T],
}

impl<T, I> IndexSlice<T, I> {
    #[ref_cast_custom]
    const fn new(values: &[T]) -> &Self;

    #[ref_cast_custom]
    const fn new_mut(values: &mut [T]) -> &mut Self;

    pub const fn len(&self) -> usize {
        self.values.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub const fn first(&self) -> Option<&T> {
        self.values.first()
    }

    pub const fn first_mut(&mut self) -> Option<&mut T> {
        self.values.first_mut()
    }

    pub const fn last(&self) -> Option<&T> {
        self.values.last()
    }

    pub const fn last_mut(&mut self) -> Option<&mut T> {
        self.values.last_mut()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.values.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.values.iter_mut()
    }
}

impl<T, I> IndexSlice<T, I>
    where I: MapIndex,
{
    pub fn get(&self, idx: impl Borrow<I>) -> Option<&T> {
        self.values.get(idx.borrow().as_usize())
    }

    pub fn get_mut(&mut self, idx: impl Borrow<I>) -> Option<&mut T> {
        self.values.get_mut(idx.borrow().as_usize())
    }

    pub fn last_index(&self) -> Option<I> {
        self.len().checked_sub(1).map(I::from_usize)
    }

    pub fn indexes(&self) -> impl Iterator<Item = I> + ExactSizeIterator + DoubleEndedIterator + use<T, I> {
        (0..self.len()).map(I::from_usize)
    }

    pub fn enumerated(&self) -> impl Iterator<Item = (I, &T)> + ExactSizeIterator + DoubleEndedIterator {
        std::iter::zip(
            self.indexes(),
            self.iter(),
        )
    }

    pub fn enumerated_mut(&mut self) -> impl Iterator<Item = (I, &mut T)> + ExactSizeIterator + DoubleEndedIterator {
        std::iter::zip(
            self.indexes(),
            self.iter_mut(),
        )
    }
}

impl<T, I> Index<I> for IndexSlice<T, I>
    where I: MapIndex,
{
    type Output = T;

    fn index(&self, index: I) -> &Self::Output {
        &self.values[index.as_usize()]
    }
}

impl<T, I> Index<&I> for IndexSlice<T, I>
    where I: MapIndex,
{
    type Output = T;

    fn index(&self, index: &I) -> &Self::Output {
        &self.values[index.as_usize()]
    }
}

impl<T, I> IndexMut<I> for IndexSlice<T, I>
    where I: MapIndex,
{
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        &mut self.values[index.as_usize()]
    }
}

impl<T, I> IndexMut<&I> for IndexSlice<T, I>
    where I: MapIndex,
{
    fn index_mut(&mut self, index: &I) -> &mut Self::Output {
        &mut self.values[index.as_usize()]
    }
}

pub struct IndexMap<T, I> {
    index: PhantomData<fn(I) -> I>,
    values: Vec<T>,
}

impl<T, I> IndexMap<T, I> {
    pub const fn new() -> Self {
        Self {
            index: PhantomData,
            values: Vec::new(),
        }
    }

    pub const fn as_slice(&self) -> &IndexSlice<T, I> {
        IndexSlice::new(self.values.as_slice())
    }

    pub const fn as_mut_slice(&mut self) -> &mut IndexSlice<T, I> {
        IndexSlice::new_mut(self.values.as_mut_slice())
    }

    pub fn into_inner(self) -> Vec<T> {
        self.values
    }

    pub fn reserve(&mut self, additional: usize) {
        self.values.reserve(additional);
    }

    pub fn pop(&mut self) -> Option<T> {
        self.values.pop()
    }
}

impl<T, I> IndexMap<T, I>
    where I: MapIndex,
{
    pub fn push(&mut self, val: T) -> I {
        let idx = self.values.len();
        self.values.push(val);
        I::from_usize(idx)
    }

    pub fn remove(&mut self, idx: impl Borrow<I>) -> T {
        self.values.remove(idx.borrow().as_usize())
    }

    pub fn swap_remove(&mut self, idx: impl Borrow<I>) -> T {
        self.values.swap_remove(idx.borrow().as_usize())
    }
}

impl<T, I> Default for IndexMap<T, I> {
    fn default() -> Self {
        Self { index: Default::default(), values: Default::default() }
    }
}

impl<T, I> Deref for IndexMap<T, I> {
    type Target = IndexSlice<T, I>;

    fn deref(&self) -> &Self::Target {
        IndexSlice::new(self.values.as_slice())
    }
}

impl<T, I> DerefMut for IndexMap<T, I> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        IndexSlice::new_mut(self.values.as_mut_slice())
    }
}
