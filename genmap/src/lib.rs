#![feature(integer_widen_truncate)]

use itertools::izip;

mod integers;
use indexmap::{IndexMap, IndexSlice, MapIndex as _};
pub use integers::*;
#[cfg(test)]
mod tests;

struct SparseCell {
    dense_idx_or_next_free: DenseOrSparse,
    generation: Generation,
}

struct AliveCellRef<'a>(&'a SparseCell);

impl AliveCellRef<'_> {
    fn dense_idx(&self) -> DenseIdx {
        self.0.dense_idx_or_next_free.as_dense()
    }
}

pub struct GenMap<T> {
    free_head: SparseIdx,
    sparse: IndexMap<SparseCell, SparseIdx>,
    dense_to_sparse: IndexMap<SparseIdx, DenseIdx>,
    dense_values: IndexMap<T, DenseIdx>,
}

impl<T> GenMap<T> {
    pub fn new() -> Self {
        Self::default()
    }

    fn get_cell(&self, handle: Handle<T>) -> Option<AliveCellRef<'_>> {
        let cell = self.sparse.get(handle.sparse_index())?;
        (cell.generation == handle.generation()).then(|| AliveCellRef(cell))
    }

    fn assume_cell_alive(&self, index: SparseIdx) -> AliveCellRef<'_> {
        AliveCellRef(&self.sparse[index])
    }

    /// Returns true if and only if the given handle is still alive
    pub fn has(&self, handle: Handle<T>) -> bool {
        self.get_cell(handle).is_some()
    }

    /// This returns the Handle to the value at the given dense index
    /// Returns None if the given index is greater or equal than the length
    /// of the array.
    pub fn from_dense_index(&self, index: DenseIdx) -> Option<Handle<T>> {
        let sparse_idx = self.dense_to_sparse.get(index)?.clone();
        let generation = self.sparse[&sparse_idx].generation;
        Some(Handle::new(sparse_idx, generation))
    }

    /// If the given sparse_index is currently inocupied, this will return
    /// an inocupied handle, using it with any method may return incoherent values
    /// or even panic
    pub fn unsafe_from_sparse_index(&self, sparse_index: SparseIdx) -> Handle<T> {
        let generation = self.sparse[&sparse_index].generation;
        Handle::new(sparse_index, generation)
    }

    /// If the given sparse_index is currently inocupied the returned index
    /// will be arbitrary, may not be a valid index for the dense array
    pub fn unsafe_get_dense_index(&self, sparse_index: SparseIdx) -> DenseIdx {
        self.assume_cell_alive(sparse_index).dense_idx()
    }

    /// If the given sparse index is currently incopuied this may
    /// panic or return any arbitrary value currently in the map
    pub fn unsafe_get(&self, sparse_index: SparseIdx) -> &T {
        let idx = self.unsafe_get_dense_index(sparse_index);
        &self.dense_values[idx]
    }

    /// See [Self::unsafe_get]
    pub fn unsafe_get_mut(&mut self, sparse_index: SparseIdx) -> &mut T {
        let idx = self.unsafe_get_dense_index(sparse_index);
        &mut self.dense_values[idx]
    }

    /// Returns the index into the dense array for the given handle.
    /// This index may change if any other value inside the map is removed.
    pub fn get_dense_index(&self, handle: Handle<T>) -> Option<DenseIdx> {
        self.get_cell(handle).map(|cell| cell.dense_idx())
    }

    /// Returns a reference to the value for the given handle if it is still valid.
    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        let idx = self.get_dense_index(handle)?;
        Some(&self.dense_values[idx])
    }

    /// Returns a mutable reference to the value for the given handle if it is still valid.
    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut T> {
        let idx = self.get_dense_index(handle)?;
        Some(&mut self.dense_values[idx])
    }

    /// Inserts the given value into the map and returns a unique handle to it.
    /// This will append the value to the end of the dense array, so no value's
    /// dense index will move.
    pub fn insert(&mut self, value: T) -> Handle<T> {
        if self.free_head.as_usize() == self.sparse.len() {
            let generation = Generation::new();
            let dense_idx = self.dense_values.push(value);
            let sparse_idx = self.sparse.push(SparseCell {
                dense_idx_or_next_free: DenseOrSparse::new_dense(dense_idx),
                generation,
            });
            self.dense_to_sparse.push(sparse_idx.clone());
            self.free_head = SparseIdx::from_usize(self.sparse.len());
            Handle::new(sparse_idx, generation)
        }
        else {
            let cell = &mut self.sparse[&self.free_head];
            let sparse_idx = std::mem::replace(&mut self.free_head, cell.dense_idx_or_next_free.as_sparse());
            let dense_idx = self.dense_values.push(value);
            self.dense_to_sparse.push(sparse_idx.clone());
            cell.dense_idx_or_next_free = DenseOrSparse::new_dense(dense_idx);
            Handle::new(sparse_idx, cell.generation)
        }
    }

    /// Removes and return the value for the given handle if it is valid.
    /// This may change the dense index of any other handle.
    pub fn remove(&mut self, handle: Handle<T>) -> Option<T> {
        let cell = self.get_cell(handle)?;
        debug_assert!(!self.dense_values.is_empty() && !self.dense_to_sparse.is_empty(), "If a handle is valid, it means there is at least one value");
        let dense_idx = cell.dense_idx();
        let sparse_idx: SparseIdx;

        let result = if self.dense_values.last_index().as_ref() == Some(&dense_idx) {
            sparse_idx = self.dense_to_sparse.pop().unwrap();
            self.dense_values.pop().unwrap()
        }
        else {
            let moved_sparse = self.dense_to_sparse.last().expect("non empty");
            self.sparse[moved_sparse].dense_idx_or_next_free = DenseOrSparse::new_dense(dense_idx.clone());
            sparse_idx = self.dense_to_sparse.swap_remove(&dense_idx);
            self.dense_values.swap_remove(&dense_idx)
        };

        let sparse = &mut self.sparse[&sparse_idx];
        sparse.generation.increment();
        sparse.dense_idx_or_next_free = DenseOrSparse::new_sparse(std::mem::replace(&mut self.free_head, sparse_idx));

        Some(result)
    }

    /// Returns a reference to the dense array.
    /// If [Self::remove] was never called, this array will be in insertion order
    pub fn values(&self) -> &IndexSlice<T, DenseIdx> {
        &self.dense_values
    }

    /// Returns a mutable reference to the dense array
    /// See [Self::value]
    pub fn values_mut(&mut self) -> &mut IndexSlice<T, DenseIdx> {
        &mut self.dense_values
    }

    /// Retunrs the amount of values in the map
    pub fn len(&self) -> usize {
        self.dense_values.len()
    }

    /// Returns an iterator of references over all values in the map
    /// in the same order as in the dense array
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.dense_values.iter()
    }

    /// Iterate over the sparse indices of the values in the same order as in
    /// the dense array.
    pub fn iter_sparse_indexes(&self) -> impl Iterator<Item = SparseIdx> + DoubleEndedIterator + ExactSizeIterator {
        self.dense_to_sparse.iter().cloned()
    }

    /// Iterate over the dense indices of the values in the same order as in
    /// the dense array.
    pub fn iter_dense_indexes(&self) -> impl Iterator<Item = DenseIdx> + DoubleEndedIterator + ExactSizeIterator {
        self.dense_values.indexes()
    }

    pub fn enumerated(&self) -> impl Iterator<Item = (SparseIdx, DenseIdx, &'_ T)> + DoubleEndedIterator + ExactSizeIterator {
        izip!(
            self.iter_sparse_indexes(),
            self.iter_dense_indexes(),
            self.iter()
        )
    }

    /// Returns an iterator of mutable references over all values in the map
    /// in the same order as in the dense array
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.dense_values.iter_mut()
    }
}

impl<T> Default for GenMap<T> {
    fn default() -> Self {
        Self {
            free_head: SparseIdx::from_usize(0),
            sparse: IndexMap::new(),
            dense_to_sparse: IndexMap::new(),
            dense_values: IndexMap::new(),
        }
    }
}
