#![feature(integer_widen_truncate)]

mod integers;
pub use integers::*;
#[cfg(test)]
mod tests;

struct SparseCell {
    dense_idx_or_next_free: DenseOrSparse,
    generation: Generation,
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

    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        let cell = self.sparse.get(handle.sparse().to_usize())?;

        if cell.generation != handle.generation() {
            None
        }
        else {
            Some(&self.dense_values[cell.dense_idx_or_next_free.as_dense().to_usize()])
        }
    }

    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut T> {
        let cell = self.sparse.get_mut(handle.sparse().to_usize())?;

        if cell.generation != handle.generation() {
            None
        }
        else {
            Some(&mut self.dense_values[cell.dense_idx_or_next_free.as_dense().to_usize()])
        }
    }

    pub fn remove(&mut self, handle: Handle<T>) -> Option<T> {
        let cell = self.sparse.get_mut(handle.sparse().to_usize())?;
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

    pub fn values(&self) -> &[T] {
        &self.dense_values
    }

    pub fn values_mut(&mut self) -> &mut [T] {
        &mut self.dense_values
    }

    pub fn len(&self) -> usize {
        self.dense_values.len()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.dense_values.iter()
    }

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
