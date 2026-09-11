#![feature(type_alias_impl_trait)]

use std::{borrow::Borrow, marker::PhantomData, ops::{Deref, DerefMut, Index, IndexMut}};
use ref_cast::{ref_cast_custom, RefCastCustom};

pub trait MapIndex {
    fn from_usize(idx: usize) -> Self;
    fn as_usize(&self) -> usize;
}

/// This is an opaque type because we cannot make faster iterators manually
/// than using the std's ones, so this opaque is actually a mapped range iterator.
pub type IndexesIteratorInner<I: MapIndex> = impl Iterator<Item = I> + ExactSizeIterator + DoubleEndedIterator + Clone + Sized + std::fmt::Debug;

pub struct IndexesIterator<I> {
    i: PhantomData<fn(I) -> I>,
    len: usize,
}

impl<I> IndexesIterator<I> {
    #[inline]
    pub fn as_with_index<O>(self) -> IndexesIterator<O> {
        IndexesIterator {
            i: PhantomData,
            len: self.len,
        }
    }

    #[inline]
    pub fn len(self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(self) -> bool {
        self.len == 0
    }
}

impl<I> Copy for IndexesIterator<I> { }
impl<I> Clone for IndexesIterator<I> { fn clone(&self) -> Self { *self } }

impl<I> std::fmt::Debug for IndexesIterator<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexesIterator").field("len", &self.len).finish()
    }
}

impl<I: MapIndex> IntoIterator for IndexesIterator<I> {
    type Item = I;
    type IntoIter = IndexesIteratorInner<I>;

    #[define_opaque(IndexesIteratorInner)]
    #[inline]
    fn into_iter(self) -> IndexesIteratorInner<I> {
        (0..self.len).map(I::from_usize)
    }
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

    #[inline]
    pub const fn as_with_index<N>(&self) -> &IndexSlice<T, N> {
        IndexSlice::<T, N>::new(&self.values)
    }

    #[inline]
    pub const fn as_with_index_mut<N>(&mut self) -> &mut IndexSlice<T, N> {
        IndexSlice::<T, N>::new_mut(&mut self.values)
    }

    #[inline]
    pub fn as_std_slice(&self) -> &[T] {
        &self.values
    }

    #[inline]
    pub fn as_std_slice_mut(&mut self) -> &mut [T] {
        &mut self.values
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.values.len()
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    #[inline]
    pub const fn first(&self) -> Option<&T> {
        self.values.first()
    }

    #[inline]
    pub const fn first_mut(&mut self) -> Option<&mut T> {
        self.values.first_mut()
    }

    #[inline]
    pub const fn last(&self) -> Option<&T> {
        self.values.last()
    }

    #[inline]
    pub const fn last_mut(&mut self) -> Option<&mut T> {
        self.values.last_mut()
    }

    #[inline]
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.values.iter()
    }

    #[inline]
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.values.iter_mut()
    }
}

impl<T, I> IndexSlice<T, I>
    where I: MapIndex,
{
    #[inline]
    pub fn get(&self, idx: impl Borrow<I>) -> Option<&T> {
        self.values.get(idx.borrow().as_usize())
    }

    #[inline]
    pub fn get_mut(&mut self, idx: impl Borrow<I>) -> Option<&mut T> {
        self.values.get_mut(idx.borrow().as_usize())
    }

    #[inline]
    pub fn last_index(&self) -> Option<I> {
        self.len().checked_sub(1).map(I::from_usize)
    }

    #[inline]
    pub fn indexes(&self) -> IndexesIterator<I> {
        IndexesIterator { i: PhantomData, len: self.len() }
    }

    #[inline]
    pub fn enumerated(&self) -> std::iter::Zip<IndexesIteratorInner<I>, std::slice::Iter<'_, T>> {
        std::iter::zip(
            self.indexes(),
            self.iter(),
        )
    }

    #[inline]
    pub fn enumerated_mut(&mut self) -> std::iter::Zip<IndexesIteratorInner<I>, std::slice::IterMut<'_, T>> {
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

    #[inline]
    fn index(&self, index: I) -> &Self::Output {
        &self.values[index.as_usize()]
    }
}

impl<T, I> Index<&I> for IndexSlice<T, I>
    where I: MapIndex,
{
    type Output = T;

    #[inline]
    fn index(&self, index: &I) -> &Self::Output {
        &self.values[index.as_usize()]
    }
}

impl<T, I> IndexMut<I> for IndexSlice<T, I>
    where I: MapIndex,
{
    #[inline]
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        &mut self.values[index.as_usize()]
    }
}

impl<T, I> IndexMut<&I> for IndexSlice<T, I>
    where I: MapIndex,
{
    #[inline]
    fn index_mut(&mut self, index: &I) -> &mut Self::Output {
        &mut self.values[index.as_usize()]
    }
}

impl<'a, T, I> IntoIterator for &'a IndexSlice<T, I> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T, I> IntoIterator for &'a mut IndexSlice<T, I> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
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

    #[inline]
    pub const fn as_slice(&self) -> &IndexSlice<T, I> {
        IndexSlice::new(self.values.as_slice())
    }

    #[inline]
    pub const fn as_mut_slice(&mut self) -> &mut IndexSlice<T, I> {
        IndexSlice::new_mut(self.values.as_mut_slice())
    }

    #[inline]
    pub fn into_with_index<N>(self) -> IndexMap<T, N> {
        IndexMap { index: PhantomData, values: self.values }
    }

    #[inline]
    pub const fn from_vec(values: Vec<T>) -> Self {
        Self {
            index: PhantomData,
            values,
        }
    }

    #[doc(alias = "into_inner")]
    #[inline]
    pub fn into_vec(self) -> Vec<T> {
        self.values
    }

    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.values.reserve(additional);
    }

    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        self.values.pop()
    }
}

impl<T, I> IndexMap<T, I>
    where I: MapIndex,
{
    #[inline]
    pub fn push(&mut self, val: T) -> I {
        let idx = self.values.len();
        self.values.push(val);
        I::from_usize(idx)
    }

    #[inline]
    pub fn remove(&mut self, idx: impl Borrow<I>) -> T {
        self.values.remove(idx.borrow().as_usize())
    }

    #[inline]
    pub fn swap_remove(&mut self, idx: impl Borrow<I>) -> T {
        self.values.swap_remove(idx.borrow().as_usize())
    }
}

impl<T, I> From<Vec<T>> for IndexMap<T, I> {
    #[inline]
    fn from(value: Vec<T>) -> Self {
        Self::from_vec(value)
    }
}

impl<T, I> From<IndexMap<T, I>> for Vec<T> {
    #[inline]
    fn from(value: IndexMap<T, I>) -> Self {
        value.into_vec()
    }
}

impl<T, I> Default for IndexMap<T, I> {
    #[inline]
    fn default() -> Self {
        Self { index: PhantomData, values: Vec::default() }
    }
}

impl<T, I> Deref for IndexMap<T, I> {
    type Target = IndexSlice<T, I>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        IndexSlice::new(self.values.as_slice())
    }
}

impl<T, I> DerefMut for IndexMap<T, I> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        IndexSlice::new_mut(self.values.as_mut_slice())
    }
}

impl<T, I> FromIterator<T> for IndexMap<T, I> {
    #[inline]
    fn from_iter<A: IntoIterator<Item = T>>(iter: A) -> Self {
        Self::from_vec(Vec::from_iter(iter))
    }
}

impl<T, I> IntoIterator for IndexMap<T, I> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.into_vec().into_iter()
    }
}

impl<'a, T, I> IntoIterator for &'a IndexMap<T, I> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T, I> IntoIterator for &'a mut IndexMap<T, I> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T, I> std::fmt::Debug for IndexMap<T, I>
    where T: std::fmt::Debug,
          I: MapIndex + std::fmt::Debug
{
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_map();
        for (k, v) in self.enumerated() {
            debug.entry(&k, v);
        }
        debug.finish()
    }
}

impl<T, I> Clone for IndexMap<T, I>
    where T: Clone,
{
    fn clone(&self) -> Self {
        Self { index: PhantomData, values: self.values.clone() }
    }
}
