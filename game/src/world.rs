use std::{collections::HashMap, sync::Arc};

use glam::{ISizeVec3, Vec3};
use parking_lot::RwLock;

use crate::{chunk::{BlockData, CHUNK_SIZE, Chunk, ChunkBlockIndex}, data_extractor::MinecraftData, resource_location::location, utils::Dda};

pub type SharedWorldRef = Arc<RwLock<World>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RayCastResult<'a> {
    pub block: &'a BlockData,
    pub pos: ISizeVec3,
    pub normal: ISizeVec3,
}

pub struct World {
    #[expect(dead_code, reason = "yet unused")]
    mcdata: Arc<MinecraftData>,
    chunks: HashMap<ISizeVec3, Arc<Chunk>>,
}

impl World {
    pub fn new(mcdata: Arc<MinecraftData>) -> Self {
        Self {
            mcdata,
            chunks: HashMap::new(),
        }
    }

    pub fn set_chunk(&mut self, pos: ISizeVec3, chunk: Arc<Chunk>) {
        self.chunks.insert(pos, chunk);
    }

    pub fn get_chunk_mut(&mut self, pos: ISizeVec3) -> Option<&mut Chunk> {
        self.chunks.get_mut(&pos).map(Arc::make_mut)
    }

    pub fn get_chunk(&self, pos: ISizeVec3) -> Option<&Arc<Chunk>> {
        self.chunks.get(&pos)
    }

    pub fn get_block(&self, pos: ISizeVec3) -> Option<&BlockData> {
        let chunk_pos = pos.div_euclid(CHUNK_SIZE.as_isizevec3());
        let chunk = self.get_chunk(chunk_pos)?;
        Some(chunk.get_data(ChunkBlockIndex::from_pos_rem_euclid(pos)))
    }

    pub fn set_block(&mut self, pos: ISizeVec3, data: &BlockData) -> bool {
        let chunk_pos = pos.div_euclid(CHUNK_SIZE.as_isizevec3());
        let Some(chunk) = self.get_chunk_mut(chunk_pos)
        else { return false; };
        chunk.set_data(ChunkBlockIndex::from_pos_rem_euclid(pos), data);
        true
    }

    pub fn cast_ray(&self, origin: Vec3, dir: Vec3, max_distance: f32) -> Option<RayCastResult<'_>> {
        let air = location!("air");

        Dda::new(origin, dir)
            .take_while(|hit| hit.t < max_distance)
            .find(|hit| self.get_block(hit.voxel).is_some_and(|p| p.id != air))
            .and_then(|hit| self.get_block(hit.voxel).map(|block| RayCastResult {
                block,
                pos: hit.voxel,
                normal: hit.normal,
            }))
    }
}
