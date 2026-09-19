
use std::sync::LazyLock;

use glam::USizeVec3;
use ordermap::{OrderSet, orderset};

use crate::{resource_location::{ResourceLocation, location}, utils::RefOrOwned};

mod block_index;
pub use block_index::*;

pub const CHUNK_SIZE: USizeVec3 = USizeVec3::new(16, 16, 16);
pub const CHUNK_BLOCK_COUNT: usize = CHUNK_SIZE.x * CHUNK_SIZE.y * CHUNK_SIZE.z;

#[derive(Debug, Clone)]
enum ChunkData {
    Filled,
    U8(Box<[u8; CHUNK_BLOCK_COUNT]>),
    U16(Box<[u16; CHUNK_BLOCK_COUNT]>),
    U32(Box<[u32; CHUNK_BLOCK_COUNT]>),
}

impl ChunkData {
    fn upgrade<T, U>(data: &[T; CHUNK_BLOCK_COUNT]) -> Box<[U; CHUNK_BLOCK_COUNT]>
        where T: Copy,
              U: From<T>,
    {
        Box::new(std::array::from_fn(|i| U::from(data[i])))
    }

    fn zeros<T: Copy + Default>() -> Box<[T; CHUNK_BLOCK_COUNT]> {
        Box::new([T::default(); CHUNK_BLOCK_COUNT])
    }

    fn set_u8(&mut self, idx: ChunkBlockIndex, value: u8) {
        match self {
            Self::Filled if value == 0 => (),
            Self::Filled => {
                let mut values = Self::zeros();
                values[idx] = value;
                *self = Self::U8(values);
            },
            Self::U8(values) => values[idx] = value,
            Self::U16(values) => values[idx] = value.into(),
            Self::U32(values) => values[idx] = value.into(),
        }
    }

    fn set_u16(&mut self, idx: ChunkBlockIndex, value: u16) {
        match self {
            Self::Filled if value == 0 => (),
            Self::Filled => {
                let mut values = Self::zeros();
                values[idx] = value;
                *self = Self::U16(values);
            },
            Self::U8(values) => {
                let mut values = Self::upgrade(values);
                values[idx] = value;
                *self = Self::U16(values);
            },
            Self::U16(values) => values[idx] = value,
            Self::U32(values) => values[idx] = value.into(),
        }
    }

    fn set_u32(&mut self, idx: ChunkBlockIndex, value: u32) {
        match self {
            Self::Filled if value == 0 => (),
            Self::Filled => {
                let mut values = Self::zeros();
                values[idx] = value;
                *self = Self::U32(values);
            },
            Self::U8(values) => {
                let mut values = Self::upgrade(values);
                values[idx] = value;
                *self = Self::U32(values);
            },
            Self::U16(values) => {
                let mut values = Self::upgrade(values);
                values[idx] = value;
                *self = Self::U32(values);
            },
            Self::U32(values) => values[idx] = value,
        }
    }

    fn set(&mut self, idx: ChunkBlockIndex, value: usize) {
        if let Ok(value) = u8::try_from(value) {
            self.set_u8(idx, value);
        }
        else if let Ok(value) = u16::try_from(value) {
            self.set_u16(idx, value);
        }
        else if let Ok(value) = u32::try_from(value) {
            self.set_u32(idx, value);
        }
        else {
            panic!("Palette index overflows u32");
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct BlockData {
    pub id: ResourceLocation,
    pub state: String,
}

#[derive(Debug, Clone)]
struct ChunkPalette {
    blocks: OrderSet<BlockData>,
}

#[derive(Debug, Clone)]
pub struct Chunk {
    blocks: ChunkData,
    pallette: ChunkPalette,
}

impl Chunk {
    #[expect(unused, reason = "unused util")]
    pub fn empty() -> &'static Self {
        static VALUE: LazyLock<Chunk> = LazyLock::new(|| Chunk {
            blocks: ChunkData::Filled,
            pallette: ChunkPalette { blocks: OrderSet::from_iter([BlockData {
                id: location!("minecraft:air"),
                state: String::new(),
            }]) },
        });
        &VALUE
    }

    pub fn new() -> Self {
        Self::new_filled(BlockData {
            id: location!("minecraft:air"),
            state: String::new(),
        })
    }

    pub fn new_filled(data: BlockData) -> Self {
        Self {
            blocks: ChunkData::Filled,
            pallette: ChunkPalette { blocks: orderset![data] },
        }
    }

    pub fn palette(&self) -> &ordermap::set::Slice<BlockData> {
        self.pallette.blocks.as_slice()
    }

    pub fn palette_insert<'c>(&mut self, data: impl RefOrOwned<'c, BlockData>) -> usize {
        if let Some(idx) = self.pallette.blocks.get_index_of(data.borrow()) {
            idx
        }
        else {
            self.pallette.blocks.insert_full(data.into_owned()).0
        }
    }

    #[expect(dead_code, reason = "yet-unused util")]
    pub fn try_get(&self, pos: USizeVec3) -> Option<usize> {
        Some(self.get(ChunkBlockIndex::from_pos(pos)?))
    }

    pub fn get(&self, idx: ChunkBlockIndex) -> usize {
        match &self.blocks {
            ChunkData::Filled => 0,
            ChunkData::U8(d) => d[idx] as usize,
            ChunkData::U16(d) => d[idx] as usize,
            ChunkData::U32(d) => d[idx] as usize,
        }
    }

    pub fn try_get_data(&self, pos: USizeVec3) -> Option<&BlockData> {
        Some(self.get_data(ChunkBlockIndex::from_pos(pos)?))
    }

    pub fn get_data(&self, pos: ChunkBlockIndex) -> &BlockData {
        &self.pallette.blocks[self.get(pos)]
    }

    pub fn set(&mut self, idx: ChunkBlockIndex, palette_idx: usize) {
        assert!(self.pallette.blocks.len() > palette_idx, "Palette index out of bound");
        self.blocks.set(idx, palette_idx);
    }

    #[expect(dead_code, reason = "yet-unused util")]
    pub fn set_data(&mut self, idx: ChunkBlockIndex, val: &BlockData) {
        let palette_idx = self.palette_insert(val);
        self.set(idx, palette_idx);
    }
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}
