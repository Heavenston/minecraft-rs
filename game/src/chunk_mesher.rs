use static_assertions as ca;

use crate::{chunk::{CHUNK_SIZE, Chunk}, resource_location::ResourceLocation, utils::CardinalDirection};

ca::const_assert!(CHUNK_SIZE.x.is_power_of_two());
ca::const_assert!(CHUNK_SIZE.y.is_power_of_two());
ca::const_assert!(CHUNK_SIZE.z.is_power_of_two());

/// First [`CHUNK_OFFSET_BITS`] bits are offset into the chunk.
/// The remaining [`CHUNK_TEXTURE_BITS`] bits index into the chunks possible textures.
type FaceInstanceData = u16;

const CHUNK_OFFSET_BITS: u32 = CHUNK_SIZE.x.ilog2() + CHUNK_SIZE.y.ilog2() + CHUNK_SIZE.z.ilog2();
const CHUNK_TEXTURE_BITS: u32 = FaceInstanceData::BITS - CHUNK_OFFSET_BITS;
ca::const_assert!(CHUNK_TEXTURE_BITS > 0);
const CHUNK_TEXTURE_COUNT: u32 = 2u32.pow(CHUNK_TEXTURE_BITS);

pub struct ChunkTexture {
    pub id: ResourceLocation,
}

pub struct ChunkSubMesh {
    pub face: CardinalDirection,
    pub textures: [ChunkTexture; CHUNK_TEXTURE_COUNT as usize],
    pub instances: Box<[FaceInstanceData]>,
}

pub struct ChunkMesh {
    pub sub_meshes: Vec<ChunkSubMesh>,
}

pub fn mesh_chunk(chunk: &Chunk) -> ChunkMesh {
    todo!()
}
