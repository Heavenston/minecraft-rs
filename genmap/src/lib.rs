#![feature(integer_widen_truncate)]

mod integers;
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
    sparse: Vec<SparseCell>,
    dense_to_sparse: Vec<SparseIdx>,
    dense_values: Vec<T>,
}

impl<T> GenMap<T> {
    pub fn new() -> Self {
        Self::default()
    }

    fn get_cell(&self, handle: Handle<T>) -> Option<AliveCellRef<'_>> {
        let cell = self.sparse.get(handle.sparse_index().to_usize())?;
        (cell.generation == handle.generation()).then(|| AliveCellRef(cell))
    }

    fn assume_cell_alive(&self, index: SparseIdx) -> AliveCellRef<'_> {
        AliveCellRef(&self.sparse[index.to_usize()])
    }

    /// Returns true if and only if the given handle is still alive
    pub fn has(&self, handle: Handle<T>) -> bool {
        self.get_cell(handle).is_some()
    }

    /// This returns the Handle to the value at the given dense index
    /// Returns None if the given index is greater or equal than the length
    /// of the array.
    pub fn from_dense_index(&self, index: usize) -> Option<Handle<T>> {
        if index >= self.dense_values.len() {
            None
        }
        else {
            let sparse_idx = self.dense_to_sparse[index].clone();
            let generation = self.sparse[sparse_idx.to_usize()].generation;
            Some(Handle::new(sparse_idx, generation))
        }
    }

    /// If the given sparse_index is currently inocupied, this will return
    /// an inocupied handle, using it with any method may return incoherent values
    /// or even panic
    pub fn unsafe_from_sparse_index(&self, sparse_index: SparseIdx) -> Option<Handle<T>> {
        let generation = self.sparse.get(sparse_index.to_usize())?.generation;
        Some(Handle::new(sparse_index, generation))
    }

    /// If the given sparse_index is currently inocupied the returned index
    /// will be arbitrary, may not be a valid index for the dense array
    pub fn unsafe_get_dense_index(&self, sparse_index: SparseIdx) -> usize {
        self.assume_cell_alive(sparse_index).dense_idx().to_usize()
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
    pub fn get_dense_index(&self, handle: Handle<T>) -> Option<usize> {
        self.get_cell(handle).map(|cell| cell.dense_idx().to_usize())
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
    /// This will append the value to the end of the dense array, so no value
    /// will move.
    pub fn insert(&mut self, value: T) -> Handle<T> {
        let dense_idx = DenseIdx::from_usize(self.dense_values.len());

        let cell_index = self.free_head.to_usize();
        if cell_index == self.sparse.len() {
            let sparse_idx = SparseIdx::from_usize(self.sparse.len());
            let generation = Generation::new();
            self.dense_values.push(value);
            self.dense_to_sparse.push(sparse_idx.clone());
            self.sparse.push(SparseCell {
                dense_idx_or_next_free: DenseOrSparse::new_dense(dense_idx),
                generation,
            });
            self.free_head = SparseIdx::from_usize(self.sparse.len());
            Handle::new(sparse_idx, generation)
        }
        else {
            let cell = &mut self.sparse[cell_index];
            let sparse_idx = std::mem::replace(&mut self.free_head, cell.dense_idx_or_next_free.as_sparse());
            self.dense_values.push(value);
            self.dense_to_sparse.push(sparse_idx.clone());
            cell.dense_idx_or_next_free = DenseOrSparse::new_dense(dense_idx);
            Handle::new(sparse_idx, cell.generation)
        }
    }

    /// Removes and return the value for the given handle if it is valid.
    /// This may change the dense index of any other handle.
    pub fn remove(&mut self, handle: Handle<T>) -> Option<T> {
        let cell = self.sparse.get_mut(handle.sparse_index().to_usize())?;
        let dense_idx = cell.dense_idx_or_next_free.as_dense();
        let dense_index = dense_idx.to_usize();
        let sparse_idx: SparseIdx;

        let result = if cell.generation != handle.generation() {
            return None;
        }
        else if self.dense_values.len() == dense_index+1 {
            sparse_idx = self.dense_to_sparse.pop().unwrap();
            self.dense_values.pop().unwrap()
        }
        else {
            let moved_sparse = self.dense_to_sparse.last().unwrap();
            self.sparse[moved_sparse.to_usize()].dense_idx_or_next_free = DenseOrSparse::new_dense(dense_idx);
            sparse_idx = self.dense_to_sparse.swap_remove(dense_index);
            self.dense_values.swap_remove(dense_index)
        };

        let sparse = &mut self.sparse[sparse_idx.to_usize()];
        sparse.generation.increment();
        sparse.dense_idx_or_next_free = DenseOrSparse::new_sparse(std::mem::replace(&mut self.free_head, sparse_idx));

        Some(result)
    }

    /// Returns a reference to the dense array.
    /// If [Self::remove] was never called, this array will be in insertion order
    pub fn values(&self) -> &[T] {
        &self.dense_values
    }

    /// Returns a mutable reference to the dense array
    /// See [Self::value]
    pub fn values_mut(&mut self) -> &mut [T] {
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
            sparse: vec![],
            dense_to_sparse: vec![],
            dense_values: vec![],
        }
    }
}
