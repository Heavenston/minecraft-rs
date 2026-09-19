use std::{hint::likely, ops::{Index, IndexMut}};

use super::{CHUNK_SIZE, CHUNK_BLOCK_COUNT};
use glam::USizeVec3;
use static_assertions as sa;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChunkBlockIndex(u16);

impl ChunkBlockIndex {
    pub fn from_pos(pos: USizeVec3) -> Option<Self> {
        likely(pos.cmplt(CHUNK_SIZE).all()).then(|| {
            // SAFETY: `pos.cmplt(CHUNK_SIZE)` 
            unsafe { Self::from_pos_unchecked(pos) }
        })
    }

    pub fn from_usize(val: usize) -> Option<Self> {
        likely(val < CHUNK_BLOCK_COUNT).then(|| {
            // SAFETY: `val < CHUNK_BLOCK_COUNT`
            Self(unsafe { u16::try_from(val).unwrap_unchecked() })
        })
    }

    /// # Safety
    /// [`val`] must be under [`CHUNK_BLOCK_COUNT`].
    pub unsafe fn from_usize_unchecked(val: usize) -> Self {
        sa::const_assert!(u16::MAX as usize >= CHUNK_BLOCK_COUNT);
        // SAFETY: Unsafe function contract, val < CHUNK_BLOCK_COUNT
        // statically checked to mean it is fits into a u16
        Self(unsafe { u16::try_from(val).unwrap_unchecked() })
    }

    /// # Safety
    /// All [`pos`] components must be under their respective values in [`CHUNK_SIZE`].
    pub unsafe fn from_pos_unchecked(pos: USizeVec3) -> Self {
        sa::const_assert_eq!(CHUNK_BLOCK_COUNT, CHUNK_SIZE.x * CHUNK_SIZE.y * CHUNK_SIZE.z);
        // SAFETY: Unsafe function contract, formula leads to a value < CHUNK_BLOCK_COUNT
        unsafe { Self::from_usize_unchecked(pos.x + (pos.y + pos.z * CHUNK_SIZE.y) * CHUNK_SIZE.x) }
    }

    pub fn as_usize(self) -> usize {
        self.0 as usize
    }

    pub fn as_pos(self) -> USizeVec3 {
        let u = self.as_usize();
        USizeVec3::new(u % CHUNK_SIZE.x, (u % (CHUNK_SIZE.x * CHUNK_SIZE.y)) / CHUNK_SIZE.x, u / (CHUNK_SIZE.x * CHUNK_SIZE.y))
    }
}

impl From<ChunkBlockIndex> for usize {
    fn from(index: ChunkBlockIndex) -> Self {
        index.as_usize()
    }
}

impl TryFrom<usize> for ChunkBlockIndex {
    type Error = ();
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Self::from_usize(value).ok_or(())
    }
}

impl TryFrom<USizeVec3> for ChunkBlockIndex {
    type Error = ();
    fn try_from(value: USizeVec3) -> Result<Self, Self::Error> {
        Self::from_pos(value).ok_or(())
    }
}

impl<T> Index<ChunkBlockIndex> for [T; CHUNK_BLOCK_COUNT] {
    type Output = T;
    fn index(&self, index: ChunkBlockIndex) -> &Self::Output {
        // SAFETY: ChunkBlockIndex is only constructed with integers < CHUNK_BLOCK_COUNT
        unsafe { self.get_unchecked(index.as_usize()) }
    }
}

impl<T> IndexMut<ChunkBlockIndex> for [T; CHUNK_BLOCK_COUNT] {
    fn index_mut(&mut self, index: ChunkBlockIndex) -> &mut Self::Output {
        // SAFETY: ChunkBlockIndex is only constructed with integers < CHUNK_BLOCK_COUNT
        unsafe { self.get_unchecked_mut(index.as_usize()) }
    }
}
