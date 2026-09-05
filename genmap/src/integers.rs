use std::marker::PhantomData;

use static_assertions as sa;

#[derive(PartialEq, Clone, Copy)]
pub struct Generation(u32);

impl Generation {
    pub(crate) fn new() -> Self {
        Self(0)
    }

    pub(crate) fn next(&self) -> Self {
        Self(match self.0.checked_add(1) {
            None => panic!("Generation overflowed"),
            Some(new_value) => new_value,
        })
    }

    pub(crate) fn increment(&mut self) {
        *self = self.next();
    }
}

#[derive(Clone)]
pub struct SparseIdx(u32);

impl SparseIdx {
    pub(crate) fn from_usize(val: usize) -> Self {
        Self(val.try_into().expect("GenMap overflowed u32 capacity"))
    }

    pub(crate) fn to_usize(&self) -> usize {
        sa::const_assert!(std::mem::size_of::<usize>() >= std::mem::size_of::<u32>());
        self.0 as usize
    }
}

pub(crate) struct DenseIdx(u32);

impl DenseIdx {
    pub(crate) fn from_usize(val: usize) -> Self {
        Self(val.try_into().expect("GenMap overflowed u32 capacity"))
    }

    pub(crate) fn to_usize(&self) -> usize {
        sa::const_assert!(std::mem::size_of::<usize>() >= std::mem::size_of::<u32>());
        self.0 as usize
    }
}

pub(crate) struct DenseOrSparse(u32);

impl DenseOrSparse {
    pub fn new_sparse(val: SparseIdx) -> Self {
        Self(val.0)
    }

    pub fn new_dense(val: DenseIdx) -> Self {
        Self(val.0)
    }

    pub fn as_sparse(&self) -> SparseIdx {
        SparseIdx(self.0)
    }

    pub fn as_dense(&self) -> DenseIdx {
        DenseIdx(self.0)
    }
}

#[repr(transparent)]
pub struct Handle<T> {
    data: PhantomData<fn(T) -> T>,
    value: u64,
}

impl<T> Handle<T> {
    pub fn new(sparse: SparseIdx, generation: Generation) -> Self {
        Self {
            data: PhantomData,
            value: u64::from(sparse.0) | (u64::from(generation.0) << 32)
        }
    }

    pub fn sparse_index(&self) -> SparseIdx {
        SparseIdx(self.value.truncate())
    }

    pub fn generation(&self) -> Generation {
        Generation((self.value >> 32).truncate())
    }
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self { data: PhantomData, value: self.value }
    }
}

impl<T> Copy for Handle<T> { }

impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T> Eq for Handle<T> {}

impl<T> std::fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handle")
            .field("sparse_index", &self.sparse_index().0)
            .field("generation", &self.generation().0)
            .finish()
    }
}
