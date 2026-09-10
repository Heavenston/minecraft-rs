
use glam::USizeVec3;
use itertools::Itertools as _;
use ordermap::{OrderSet, orderset};

use crate::resource_location::{ResourceLocation, location};

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
    fn upgrade_to_u16(data: &[u8; CHUNK_BLOCK_COUNT]) -> Box<[u16; CHUNK_BLOCK_COUNT]> {
        Box::new(data.iter().copied().map(From::from).collect_array().expect("Same length"))
    }

    #[expect(clippy::single_call_fn, reason = "needed once")]
    fn upgrade_to_u32(data: &[u16; CHUNK_BLOCK_COUNT]) -> Box<[u32; CHUNK_BLOCK_COUNT]> {
        Box::new(data.iter().copied().map(From::from).collect_array().expect("Same length"))
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

    pub fn set(&mut self, pos: USizeVec3, val: &BlockData) {
        let idx = Self::pos_to_idx(pos).expect("Out of bound chunk position");
        match &mut self.blocks {
            ChunkData::Filled => if self.pallette.blocks.first() == Some(val) {
                /* NOOP */
            } else {
                let mut new_blocks = Box::new([0u8; CHUNK_BLOCK_COUNT]);
                new_blocks[idx] = 1;
                self.blocks = ChunkData::U8(new_blocks);
                self.pallette.blocks.insert(val.clone());
                debug_assert_eq!(self.pallette.blocks.len(), 2);
            },
            ChunkData::U8(blocks) => {
                let (palette_idx, had) = self.pallette.blocks.insert_full(val.clone());
                if palette_idx > 0xFF {
                    debug_assert!(!had);
                    debug_assert_eq!(palette_idx, 0x0100);
                    let mut blocks = ChunkData::upgrade_to_u16(blocks);
                    blocks[idx] = 0x0100;
                    self.blocks = ChunkData::U16(blocks);
                }
                else {
                    blocks[idx] = u8::try_from(palette_idx).expect("plette_idx <= 0xFF in this branch");
                }
            },
            ChunkData::U16(blocks) => {
                let (palette_idx, had) = self.pallette.blocks.insert_full(val.clone());
                if palette_idx > 0xFFFF {
                    debug_assert!(!had);
                    debug_assert_eq!(palette_idx, 0x0001_0000);
                    let mut blocks = ChunkData::upgrade_to_u32(blocks);
                    blocks[idx] = 0x0001_0000;
                    self.blocks = ChunkData::U32(blocks);
                }
                else {
                    blocks[idx] = u16::try_from(palette_idx).expect("palette_idx <= 0xFFFF in this branch");
                }
            },
            ChunkData::U32(blocks) => {
                let (palette_idx, _had) = self.pallette.blocks.insert_full(val.clone());
                if let Ok(palette_idx) = u32::try_from(palette_idx) {
                    blocks[idx] = palette_idx;
                }
                else {
                    panic!("Exceeded u32 index capacity");
                }
            },
        }
    }
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}
