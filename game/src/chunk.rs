
use std::sync::LazyLock;

use glam::USizeVec3;
use itertools::Itertools as _;
use ordermap::{OrderSet, orderset};

use crate::{resource_location::{ResourceLocation, location}, utils::RefOrOwned};

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
    #[expect(clippy::single_call_fn, reason = "needed once")]
    fn upgrade_to_u16<T>(data: &[T; CHUNK_BLOCK_COUNT]) -> Box<[u16; CHUNK_BLOCK_COUNT]>
        where T: Copy,
              u16: From<T>,
    {
        Box::new(data.iter().copied().map(From::from).collect_array().expect("Same length"))
    }

    fn upgrade_to_u32<T>(data: &[T; CHUNK_BLOCK_COUNT]) -> Box<[u32; CHUNK_BLOCK_COUNT]>
        where T: Copy,
              u32: From<T>,
    {
        Box::new(data.iter().copied().map(From::from).collect_array().expect("Same length"))
    }

    fn set_u8(&mut self, idx: usize, value: u8) {
        match self {
            Self::Filled if value == 0 => (),
            Self::Filled => {
                let mut values: Box<[u8; CHUNK_BLOCK_COUNT]> = Box::new(std::iter::repeat_n(0, CHUNK_BLOCK_COUNT).collect_array().expect("correct length"));
                values[idx] = value;
                *self = Self::U8(values);
            },
            Self::U8(values) => values[idx] = value,
            Self::U16(values) => values[idx] = value.into(),
            Self::U32(values) => values[idx] = value.into(),
        }
    }

    fn set_u16(&mut self, idx: usize, value: u16) {
        match self {
            Self::Filled if value == 0 => (),
            Self::Filled => {
                let mut values: Box<[u16; CHUNK_BLOCK_COUNT]> = Box::new([0; CHUNK_BLOCK_COUNT]);
                values[idx] = value;
                *self = Self::U16(values);
            },
            Self::U8(values) => {
                let mut values = Self::upgrade_to_u16(values);
                values[idx] = value;
                *self = Self::U16(values);
            },
            Self::U16(values) => values[idx] = value,
            Self::U32(values) => values[idx] = value.into(),
        }
    }

    fn set_u32(&mut self, idx: usize, value: u32) {
        match self {
            Self::Filled if value == 0 => (),
            Self::Filled => {
                let mut values: Box<[u32; CHUNK_BLOCK_COUNT]> = Box::new([0; CHUNK_BLOCK_COUNT]);
                values[idx] = value;
                *self = Self::U32(values);
            },
            Self::U8(values) => {
                let mut values = Self::upgrade_to_u32(values);
                values[idx] = value;
                *self = Self::U32(values);
            },
            Self::U16(values) => {
                let mut values = Self::upgrade_to_u32(values);
                values[idx] = value;
                *self = Self::U32(values);
            },
            Self::U32(values) => values[idx] = value,
        }
    }

    fn set(&mut self, idx: usize, value: usize) {
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

    fn pos_to_idx(pos: USizeVec3) -> Option<usize> {
        pos.cmplt(CHUNK_SIZE).all().then_some(pos.x + (pos.y + pos.z * CHUNK_SIZE.y) * CHUNK_SIZE.x)
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

    pub fn try_get(&self, pos: USizeVec3) -> Option<usize> {
        let idx = Self::pos_to_idx(pos)?;
        Some(match &self.blocks {
            ChunkData::Filled => 0,
            ChunkData::U8(d) => d[idx] as usize,
            ChunkData::U16(d) => d[idx] as usize,
            ChunkData::U32(d) => d[idx] as usize,
        })
    }

    pub fn get(&self, pos: USizeVec3) -> usize {
        self.try_get(pos).expect("Out of bound chunk position")
    }

    pub fn try_get_data(&self, pos: USizeVec3) -> Option<&BlockData> {
        Some(&self.pallette.blocks[self.try_get(pos)?])
    }

    pub fn get_data(&self, pos: USizeVec3) -> &BlockData {
        self.try_get_data(pos).expect("Out of bound chunk position")
    }

    pub fn set(&mut self, pos: USizeVec3, palette_idx: usize) {
        assert!(self.pallette.blocks.len() > palette_idx, "Palette index out of bound");
        let idx = Self::pos_to_idx(pos).expect("Out of bound chunk position");
        self.blocks.set(idx, palette_idx);
    }

    #[expect(dead_code, reason = "yet-unused util")]
    pub fn set_data(&mut self, pos: USizeVec3, val: &BlockData) {
        let palette_idx = self.palette_insert(val);
        self.set(pos, palette_idx);
    }
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}
