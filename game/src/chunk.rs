use std::collections::HashMap;

use glam::USizeVec3;
use itertools::Itertools;
use ordermap::{OrderSet, orderset};

use crate::resource_location::ResourceLocation;

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
    fn upgrade_to_u16(data: &[u8; CHUNK_BLOCK_COUNT]) -> Box<[u16; CHUNK_BLOCK_COUNT]> {
        Box::new(data.iter().copied().map(From::from).collect_array().expect("Same length"))
    }

    fn upgrade_to_u32(data: &[u16; CHUNK_BLOCK_COUNT]) -> Box<[u32; CHUNK_BLOCK_COUNT]> {
        Box::new(data.iter().copied().map(From::from).collect_array().expect("Same length"))
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct BlockData {
    id: ResourceLocation,
    state: String,
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
    pub fn new_filled(data: BlockData) -> Self {
        Self {
            blocks: ChunkData::Filled,
            pallette: ChunkPalette { blocks: orderset![data] },
        }
    }

    fn pos_to_idx(pos: USizeVec3) -> Option<usize> {
        pos.cmplt(CHUNK_SIZE).all().then_some(pos.x + (pos.y + pos.z * CHUNK_SIZE.y) * CHUNK_SIZE.x)
    }

    pub fn try_get(&self, pos: USizeVec3) -> Option<&BlockData> {
        let idx = Self::pos_to_idx(pos)?;
        let palette_idx = match &self.blocks {
            ChunkData::Filled => 0,
            ChunkData::U8(d) => d[idx] as usize,
            ChunkData::U16(d) => d[idx] as usize,
            ChunkData::U32(d) => d[idx] as usize,
        };
        Some(self.pallette.blocks.get_index(palette_idx).expect("Stored pallette index is valid"))
    }

    pub fn get(&self, pos: USizeVec3) -> &BlockData {
        self.try_get(pos).expect("Out of bound chunk position")
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
                debug_assert_eq!(self.pallette.blocks.len(), 2)
            },
            ChunkData::U8(blocks) => {
                let (palette_idx, had) = self.pallette.blocks.insert_full(val.clone());
                if palette_idx > 255 {
                    debug_assert!(!had);
                    debug_assert_eq!(palette_idx, 256);
                    let mut blocks = ChunkData::upgrade_to_u16(blocks);
                    blocks[idx] = palette_idx as u16;
                    self.blocks = ChunkData::U16(blocks);
                }
                else {
                    blocks[idx] = palette_idx as u8;
                }
            },
            ChunkData::U16(blocks) => {
                let (palette_idx, had) = self.pallette.blocks.insert_full(val.clone());
                if palette_idx > 65535 {
                    debug_assert!(!had);
                    debug_assert_eq!(palette_idx, 65536);
                    let mut blocks = ChunkData::upgrade_to_u32(blocks);
                    blocks[idx] = palette_idx as u32;
                    self.blocks = ChunkData::U32(blocks);
                }
                else {
                    blocks[idx] = palette_idx as u16;
                }
            },
            ChunkData::U32(blocks) => {
                let (palette_idx, _had) = self.pallette.blocks.insert_full(val.clone());
                debug_assert!(palette_idx <= u32::MAX as usize);
                blocks[idx] = palette_idx as u32;
            },
        }
    }
}
