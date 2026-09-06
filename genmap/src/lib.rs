#![feature(integer_widen_truncate)]

use itertools::izip;

mod integers;
use indexmap::{IndexMap, IndexSlice, MapIndex as _};
pub use integers::*;
#[cfg(test)]
mod tests;

trait Sealed { }

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

pub struct SlotRef<'a, T> {
    map: &'a GenMap<T>,
    sparse_idx: SparseIdx,
    dense_idx: DenseIdx,
}

impl<'a, T> SlotRef<'a, T> {
    pub fn sparse_idx(&self) -> SparseIdx {
        self.sparse_idx
    }

    pub fn dense_idx(&self) -> DenseIdx {
        self.dense_idx
    }

    pub fn handle(&self) -> Handle<T> {
        Handle::new(self.sparse_idx, self.map.sparse[self.sparse_idx].generation)
    }

    pub fn get(&self) -> &'a T {
        &self.map.dense_values[self.dense_idx()]
    }
}

pub struct SlotMut<'a, T> {
    map: &'a mut GenMap<T>,
    sparse_idx: SparseIdx,
    dense_idx: DenseIdx,
}

impl<'a, T> SlotMut<'a, T> {
    pub fn into_ref(self) -> SlotRef<'a, T> {
        SlotRef { map: self.map, sparse_idx: self.sparse_idx, dense_idx: self.dense_idx }
    }

    pub fn sparse_idx(&self) -> SparseIdx {
        self.sparse_idx
    }

    pub fn dense_idx(&self) -> DenseIdx {
        self.dense_idx
    }

    pub fn handle(&self) -> Handle<T> {
        Handle::new(self.sparse_idx, self.map.sparse[self.sparse_idx].generation)
    }

    pub fn get(&self) -> &T {
        &self.map.dense_values[self.dense_idx()]
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.map.dense_values[self.dense_idx]
    }

    pub fn into_mut(self) -> &'a mut T {
        &mut self.map.dense_values[self.dense_idx]
    }

    /// Removes and return the value for the given handle if it is valid.
    /// This may change the dense index of any other handle.
    pub fn remove(self) -> T {
        let Self { map, sparse_idx, dense_idx } = self;
        debug_assert!(!map.dense_values.is_empty() && !map.dense_to_sparse.is_empty(), "If a handle is valid, it means there is at least one value");

        map.dense_to_sparse.swap_remove(dense_idx);
        let result = map.dense_values.swap_remove(dense_idx);

        if let Some(moved_sparse) = map.dense_to_sparse.get(dense_idx) {
            map.sparse[moved_sparse].dense_idx_or_next_free = DenseOrSparse::new_dense(dense_idx);
        }

        let cell = &mut map.sparse[&sparse_idx];
        cell.generation.increment();
        cell.dense_idx_or_next_free = DenseOrSparse::new_sparse(std::mem::replace(&mut map.free_head, sparse_idx));

        result
    }
}

#[expect(private_bounds, reason = "Sealed trait")]
#[diagnostic::on_unimplemented(message = "Use the types genmap::Handle, genmap::DenseIndex or genmap::AssumeAlive")]
pub trait GenMapIndex<T>: Sealed {
    type Checked<U>;
    fn map_checked<U, V>(val: Self::Checked<U>, mapper: impl FnOnce(U) -> V) -> Self::Checked<V>;
    fn get(map: &GenMap<T>, handle: Self) -> Self::Checked<SlotRef<'_, T>>;
    fn get_mut(map: &mut GenMap<T>, handle: Self) -> Self::Checked<SlotMut<'_, T>>;
}

impl<T> Sealed for Handle<T> { }
impl<T> GenMapIndex<T> for Handle<T> {
    type Checked<U> = Option<U>;

    fn map_checked<U, V>(val: Option<U>, mapper: impl FnOnce(U) -> V) -> Option<V> {
        Option::map(val, mapper)
    }

    fn get(map: &GenMap<T>, handle: Self) -> Option<SlotRef<'_, T>> {
        let cell = map.get_cell(handle)?;
        Some(SlotRef {
            map,
            sparse_idx: handle.sparse_index(),
            dense_idx: cell.dense_idx(),
        })
    }

    fn get_mut(map: &mut GenMap<T>, handle: Self) -> Option<SlotMut<'_, T>> {
        let cell = map.get_cell(handle)?;
        Some(SlotMut {
            sparse_idx: handle.sparse_index(),
            dense_idx: cell.dense_idx(),
            map,
        })
    }
}

impl Sealed for DenseIdx { }
impl<T> GenMapIndex<T> for DenseIdx {
    type Checked<U> = Option<U>;

    fn map_checked<U, V>(val: Option<U>, mapper: impl FnOnce(U) -> V) -> Option<V> { Option::map(val, mapper) }

    fn get(map: &GenMap<T>, handle: Self) -> Option<SlotRef<'_, T>> {
        Some(SlotRef {
            map,
            sparse_idx: *map.dense_to_sparse.get(handle)?,
            dense_idx: handle,
        })
    }

    fn get_mut(map: &mut GenMap<T>, handle: Self) -> Option<SlotMut<'_, T>> {
        Some(SlotMut {
            sparse_idx: *map.dense_to_sparse.get(handle)?,
            dense_idx: handle,
            map,
        })
    }
}

pub struct AssumeAlive(pub SparseIdx);
impl Sealed for AssumeAlive { }
impl<T> GenMapIndex<T> for AssumeAlive {
    type Checked<U> = U;

    fn map_checked<U, V>(val: U, mapper: impl FnOnce(U) -> V) -> V { mapper(val) }

    fn get(map: &GenMap<T>, Self(sparse_idx): Self) -> Self::Checked<SlotRef<'_, T>> {
        SlotRef {
            map,
            sparse_idx,
            dense_idx: map.assume_cell_alive(sparse_idx).dense_idx(),
        }
    }

    fn get_mut(map: &mut GenMap<T>, Self(sparse_idx): Self) -> Self::Checked<SlotMut<'_, T>> {
        SlotMut {
            sparse_idx,
            dense_idx: map.assume_cell_alive(sparse_idx).dense_idx(),
            map,
        }
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
        (cell.generation == handle.generation()).then_some(AliveCellRef(cell))
    }

    fn assume_cell_alive(&self, index: SparseIdx) -> AliveCellRef<'_> {
        AliveCellRef(&self.sparse[index])
    }

    /// Returns true if and only if the given handle is still alive.
    pub fn has(&self, handle: Handle<T>) -> bool {
        self.get_cell(handle).is_some()
    }

    pub fn with<I: GenMapIndex<T>>(&self, handle: I) -> I::Checked<SlotRef<'_, T>> {
        I::get(self, handle)
    }

    pub fn with_mut<I: GenMapIndex<T>>(&mut self, handle: I) -> I::Checked<SlotMut<'_, T>> {
        I::get_mut(self, handle)
    }

    /// Returns a reference to the value for the given handle if it is still valid.
    pub fn get<I: GenMapIndex<T>>(&self, handle: I) -> I::Checked<&T> {
        I::map_checked(self.with(handle), |slot| slot.get())
    }

    /// Returns a mutable reference to the value for the given handle if it is still valid.
    pub fn get_mut<I: GenMapIndex<T>>(&mut self, handle: I) -> I::Checked<&mut T> {
        I::map_checked(self.with_mut(handle), SlotMut::into_mut)
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
            self.dense_to_sparse.push(sparse_idx);
            self.free_head = SparseIdx::from_usize(self.sparse.len());
            Handle::new(sparse_idx, generation)
        }
        else {
            let cell = &mut self.sparse[&self.free_head];
            let sparse_idx = std::mem::replace(&mut self.free_head, cell.dense_idx_or_next_free.as_sparse());
            let dense_idx = self.dense_values.push(value);
            self.dense_to_sparse.push(sparse_idx);
            cell.dense_idx_or_next_free = DenseOrSparse::new_dense(dense_idx);
            Handle::new(sparse_idx, cell.generation)
        }
    }

    pub fn remove<I: GenMapIndex<T>>(&mut self, handle: I) -> I::Checked<T> {
        I::map_checked(self.with_mut(handle), SlotMut::remove)
    }

    /// Returns a reference to the dense array.
    /// If [`Self::remove`] was never called, this array will be in insertion order.
    pub fn values(&self) -> &IndexSlice<T, DenseIdx> {
        &self.dense_values
    }

    /// Returns a mutable reference to the dense array.
    /// See [`Self::value`].
    pub fn values_mut(&mut self) -> &mut IndexSlice<T, DenseIdx> {
        &mut self.dense_values
    }

    /// Retunrs the amount of values in the map.
    pub fn len(&self) -> usize {
        self.dense_values.len()
    }

    /// Returns true if [`Self::len`] returns 0.
    pub fn is_empty(&self) -> bool {
        self.dense_values.is_empty()
    }

    /// Returns an iterator of references over all values in the map
    /// in the same order as in the dense array.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.dense_values.iter()
    }

    /// Iterate over the sparse indices of the values in the same order as in
    /// the dense array.
    pub fn iter_sparse_indexes(&self) -> impl Iterator<Item = SparseIdx> + DoubleEndedIterator + ExactSizeIterator {
        self.dense_to_sparse.iter().copied()
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
    /// in the same order as in the dense array.
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

impl<'a, T> IntoIterator for &'a GenMap<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.dense_values.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut GenMap<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.dense_values.iter_mut()
    }
}
