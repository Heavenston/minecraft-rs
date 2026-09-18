use crate::utils::{ CardinalDirection, EnumSet };

#[bitfield_struct::bitfield(u32, order = Lsb)]
#[derive(bytemuck::Pod, bytemuck::Zeroable, PartialEq, Eq, Hash)]
pub struct BlockModelFaceData {
    #[bits(2)]
    pub tint_index: u32,
    #[bits(7)]
    pub texture_index: usize,
    #[bits(3)]
    pub face_tint: usize,
    #[bits(20)]
    pub _padding: (),
}

#[bitfield_struct::bitfield(u32, order = Lsb)]
#[derive(bytemuck::Pod, bytemuck::Zeroable, PartialEq, Eq, Hash)]
pub struct FaceDataCombined {
    #[bits(12)]
    pub face1: BlockModelFaceData,
    #[bits(12)]
    pub face2: BlockModelFaceData,
    #[bits(8)]
    pub _padding: (),
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct BlockModel {
    pub face_data: [FaceDataCombined; 3],
}

impl BlockModel {
    pub fn set_face(&mut self, dir: CardinalDirection, data: BlockModelFaceData) {
        let idx = enum_map::Enum::into_usize(dir);
        match idx & 1usize {
            0 => self.face_data[idx >> 1usize].set_face1(data),
            1 => self.face_data[idx >> 1usize].set_face2(data),
            _ => unreachable!(),
        }
    }
}

#[bitfield_struct::bitfield(u32, order = Lsb)]
#[derive(bytemuck::Pod, bytemuck::Zeroable)]
pub struct Block {
    #[bits(16)]
    pub model_idx: usize,
    #[bits(6)]
    pub face_mask: EnumSet<CardinalDirection>,
    #[bits(6)]
    pub group_face_mask: EnumSet<CardinalDirection>,
    #[bits(4)]
    pub _padding: (),
}

#[derive(Debug, Clone)]
pub struct Mesh {
    pub transparency: super::ChunkTransparencyMode,
    pub data: Box<[Block]>,
}

#[derive(Default)]
pub(super) struct IncompleteMesh {
    pub data: Vec<Block>,
}
