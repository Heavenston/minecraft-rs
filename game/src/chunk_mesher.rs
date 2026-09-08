use std::{mem::take, ops::Sub};

use arrayvec::ArrayVec;
use enum_map::EnumMap;
use glam::{U16Vec3, USizeVec3};
use itertools::Itertools as _;
use ordermap::OrderSet;
use static_assertions as ca;

use crate::{chunk::{BlockData, CHUNK_SIZE, Chunk}, data_extractor::{MinecraftData, blockstate::{BlockState, ModelChoice}, model::Texture}, resource_location::{ResourceLocation, location}, utils::{CardinalDirection, Vec3Range}};

ca::const_assert!(CHUNK_SIZE.x.is_power_of_two());
ca::const_assert!(CHUNK_SIZE.y.is_power_of_two());
ca::const_assert!(CHUNK_SIZE.z.is_power_of_two());

/// First [`CHUNK_OFFSET_BITS`] bits are offset into the chunk.
type FaceInstanceData = u16;

const CHUNK_OFFSET_BITS: u32 = CHUNK_SIZE.x.ilog2() + CHUNK_SIZE.y.ilog2() + CHUNK_SIZE.z.ilog2();

fn create_face_instance_data(offset: USizeVec3) -> FaceInstanceData {
    debug_assert_eq!(offset.as_u16vec3().as_usizevec3(), offset);
    let offset = offset.as_u16vec3();
    ((offset.x << CHUNK_SIZE.y.ilog2()) | offset.y) << CHUNK_SIZE.z.ilog2() | offset.z
}

const fn create_interior_ranges() -> EnumMap<CardinalDirection, Vec3Range> {
    const fn range_for_direction(direction: CardinalDirection) -> Vec3Range {
        let mut interior_min = USizeVec3::ZERO;
        let mut interior_max = CHUNK_SIZE;
        if direction.is_positive() {
            interior_max[direction.axis()] -= 1;
        }
        else {
            interior_min[direction.axis()] += 1;
        }
        Vec3Range(interior_min, interior_max)
    }
    EnumMap::from_array(CardinalDirection::VALUES.map(range_for_direction))
}
static INTERIOR_RANGES: EnumMap<CardinalDirection, Vec3Range> = create_interior_ranges();
const fn create_exterior_ranges() -> EnumMap<CardinalDirection, Vec3Range> {
    const fn range_for_direction(direction: CardinalDirection) -> Vec3Range {
        let mut interior_min = USizeVec3::ZERO;
        let mut interior_max = CHUNK_SIZE;
        if direction.is_positive() {
            interior_min[direction.axis()] = interior_max[direction.axis()] - 1;
        }
        else {
            interior_max[direction.axis()] = interior_min[direction.axis()] + 1;
        }
        Vec3Range(interior_min, interior_max)
    }
    EnumMap::from_array(CardinalDirection::VALUES.map(range_for_direction))
}
static EXTERIOR_RANGES: EnumMap<CardinalDirection, Vec3Range> = create_exterior_ranges();

pub struct ChunkSubMesh<'mc> {
    pub face: CardinalDirection,
    pub texture: &'mc ResourceLocation,
    pub instances: Box<[FaceInstanceData]>,
}

struct IncompleteSubMesh<'mc> {
    texture: &'mc ResourceLocation,
    instances: Vec<FaceInstanceData>,
}

#[derive(Default)]
struct SubMeshBuilder<'mc> {
    submeshes: EnumMap<CardinalDirection, Vec<IncompleteSubMesh<'mc>>>,
}

impl<'mc> SubMeshBuilder<'mc> {
    fn push_face(&mut self, face: CardinalDirection, pos: USizeVec3, texture: &'mc ResourceLocation) {
        let submesh = if let Some(submesh) = self.submeshes[face].iter_mut().find(|p| p.texture == texture) {
            submesh
        } else {
            self.submeshes[face].push_mut(IncompleteSubMesh { texture, instances: vec![] })
        };
        submesh.instances.push(create_face_instance_data(pos));
    }

    fn finish(self) -> Box<[ChunkSubMesh<'mc>]> {
        self.submeshes.into_iter()
            .flat_map(|(face, meshes)| meshes.into_iter().map(move |submesh| (face, submesh)))
            .map(|(face, IncompleteSubMesh { texture, instances })| {
                ChunkSubMesh {
                    face,
                    texture,
                    instances: instances.into_boxed_slice(),
                }
            })
            .collect()
    }
}

pub struct ChunkMesh<'mc> {
    pub sub_meshes: Box<[ChunkSubMesh<'mc>]>,
}

struct ChunkMesher<'mc, 'chunk, 'neighbor> {
    mcdata: &'mc MinecraftData,
    chunk: &'chunk Chunk,
    neighbors: EnumMap<CardinalDirection, &'neighbor Chunk>,

    builder: SubMeshBuilder<'mc>,
}

impl<'mc> ChunkMesher<'mc, '_, '_> {
    fn get_block_texture(&self, block_data: &BlockData) -> Option<&'mc ResourceLocation> {
        let blockstate = self.mcdata.blockstate(&block_data.id);

        let model_choice = match blockstate {
            BlockState::Variants { variants } => variants.get(&block_data.state).expect("valid block states"),
            BlockState::Multipart { .. } => panic!("Unsuported multipart blocks"),
        };
        let blockstate_model = match model_choice {
            ModelChoice::Single(model) => model,
            ModelChoice::Multiple(models) => models.first().expect("at leats one model"),
        };
        if blockstate_model.location == location!("minecraft:block/air") {
            return None;
        }
        let model = self.mcdata.model(&blockstate_model.location);
        assert_eq!(blockstate_model.x, 0, "Model rotation not suported");
        assert_eq!(blockstate_model.y, 0, "Model rotation not suported");
        assert_eq!(blockstate_model.z, 0, "Model rotation not suported");
        assert_eq!(model.parent.as_ref(), Some(&location!("minecraft:block/cube_all")), "Only the minecraft:block/cube_all model is suported");

        let texture = model.textures.get("all").expect("minecraft:block/cube_all needs an 'all' texture");
        match texture {
            Texture::Reference(reference) => panic!("Unknown texture reference {reference}"),
            Texture::Detailed { sprite: _, force_translucent: true } => panic!("Unsuported transslucent textures"),
            Texture::Detailed { sprite: resource_location, force_translucent: false } | Texture::Location(resource_location) => Some(resource_location),
        }
    }

    fn is_transparent(&self, pos: USizeVec3) -> bool {
        self.chunk.get(pos).id == location!("minecraft:air")
    }

    fn is_transparent_neighbor(&self, neighbor: CardinalDirection, pos: USizeVec3) -> bool {
        self.neighbors[neighbor].get(pos).id == location!("minecraft:air")
    }

    fn mesh_for_direction(&mut self, direction: CardinalDirection) {
        for pos in INTERIOR_RANGES[direction] {
            let neighbor = pos + direction;
            if !self.is_transparent(neighbor) { continue }
            let Some(texture) = self.get_block_texture(self.chunk.get(pos))
            else { continue };
            self.builder.push_face(direction, pos, texture);
        }

        for pos in EXTERIOR_RANGES[direction] {
            let neighbor = {
                let mut neighbor = pos;
                if direction.is_positive() {
                    neighbor[direction.axis()] = 0;
                }
                else {
                    neighbor[direction.axis()] = CHUNK_SIZE[direction.axis()]-1;
                }
                neighbor
            };

            if !self.is_transparent_neighbor(direction, neighbor) { continue }
            let Some(texture) = self.get_block_texture(self.chunk.get(pos))
            else { continue };
            self.builder.push_face(direction, pos, texture);
        }
    }
}

pub fn mesh_chunk<'mc>(mcdata: &'mc MinecraftData, chunk: &Chunk, neighbors: EnumMap<CardinalDirection, &Chunk>) -> ChunkMesh<'mc> {
    let mut mesher = ChunkMesher {
        mcdata,
        chunk,
        neighbors,
        builder: SubMeshBuilder::default(),
    };
    for dir in CardinalDirection::VALUES {
        mesher.mesh_for_direction(dir);
    }
    ChunkMesh {
        sub_meshes: mesher.builder.finish(),
    }
}
