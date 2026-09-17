use crate::utils::{ CardinalDirection, EnumSet };

#[bitfield_struct::bitfield(u32, order = Lsb)]
#[derive(bytemuck::Pod, bytemuck::Zeroable, PartialEq, Eq)]
pub struct BlockModelFaceData {
    #[bits(2)]
    pub tint_index: u32,
    #[bits(7)]
    pub texture_index: usize,
    #[bits(3)]
    pub face_tint: usize,
    #[bits(20)]
    pub _padding: usize,
}

#[bitfield_struct::bitfield(u32, order = Lsb)]
#[derive(bytemuck::Pod, bytemuck::Zeroable, PartialEq, Eq)]
pub struct FaceDataCombined {
    #[bits(12)]
    pub face1: BlockModelFaceData,
    #[bits(12)]
    pub face2: BlockModelFaceData,
    #[bits(8)]
    pub _padding: (),
}

#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct BlockModel {
    pub face_data: [FaceDataCombined; 3],
}

#[bitfield_struct::bitfield(u32, order = Lsb)]
#[derive(bytemuck::Pod, bytemuck::Zeroable)]
pub struct Block {
    #[bits(16)]
    pub model_idx: usize,
    #[bits(6)]
    pub face_mask: EnumSet<CardinalDirection>,
    #[bits(10)]
    pub _padding: usize,
}

#[derive(Debug, Clone)]
pub struct Mesh {
    pub transparency: super::ChunkTransparencyMode,
    pub data: Box<[Block]>,
    pub models: Box<[BlockModel]>,
}

#[derive(Default)]
pub(super) struct IncompleteMesh {
    pub data: Vec<Block>,
    pub models: Vec<BlockModel>,
}
