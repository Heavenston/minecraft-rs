
use std::collections::HashMap;

use enum_map::{EnumMap, enum_map};
use enumflags2::BitFlags;
use glam::{Quat, USizeVec3, Vec2, Vec3};
use ordermap::OrderSet;
use static_assertions as ca;

use crate::{chunk::{BlockData, CHUNK_SIZE, Chunk}, data_extractor::{MinecraftData, blockstate::{BlockState, ModelChoice}, model::{self, Texture}}, resource_location::{ResourceLocation, ResourceLocationOrderSet, location}, utils::{CardinalDirection, Vec3Range}};

ca::const_assert!(CHUNK_SIZE.x.is_power_of_two());
ca::const_assert!(CHUNK_SIZE.y.is_power_of_two());
ca::const_assert!(CHUNK_SIZE.z.is_power_of_two());
const CHUNK_OFFSET_BITS: u32 = CHUNK_SIZE.x.ilog2() + CHUNK_SIZE.y.ilog2() + CHUNK_SIZE.z.ilog2();
ca::const_assert!(CHUNK_OFFSET_BITS == 12);

type FaceInstanceData = u32;

fn create_face_instance_data(offset: USizeVec3) -> FaceInstanceData {
    debug_assert_eq!(offset.as_uvec3().as_usizevec3(), offset);
    let offset = offset.as_uvec3();
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

/// Sub mesh for a chunk that only contains full block faces
pub struct QuadSubMesh {
    pub direction: CardinalDirection,
    pub texture: ResourceLocation,
    pub instances: Box<[FaceInstanceData]>,
}

pub struct FullVertex {
    pub pos: Vec3,
    pub uv: Vec2,
    pub texture: u32,
}

pub struct SubMesh {
    pub vertices: Vec<FullVertex>,
}

struct IncompleteQuadSubMesh {
    texture: ResourceLocation,
    instances: Vec<FaceInstanceData>,
}

struct IncompleteSubMeshInstance {
    model: usize,
    culled: BitFlags<CardinalDirection>,
}

struct IncompleteSubMesh {
    models: Vec<usize>,
    instances: Vec<IncompleteSubMeshInstance>,
}

pub struct ChunkMesh {
    pub quad_sub_meshes: Box<[QuadSubMesh]>,
    pub sub_meshes: Box<[SubMesh]>,
}

struct ResolvedElements<'a> {
    model: ResourceLocation,
    elements: &'a [model::Element],
    textures: HashMap<String, ResourceLocation>,
}

struct FullBlockFace {
    direction: CardinalDirection,
    texture: ResourceLocation,
}

struct BlockModel {
    culling_directions: BitFlags<CardinalDirection>,
    full_block_faces: Box<[FullBlockFace]>,
}

struct ChunkMesher<'mc, 'chunk, 'neighbor> {
    mcdata: &'mc MinecraftData,
    chunk: &'chunk Chunk,
    neighbors: EnumMap<CardinalDirection, &'neighbor Chunk>,

    block_models: Vec<BlockModel>,
    textures: ResourceLocationOrderSet<ResourceLocation>,
    quad_submeshes: EnumMap<CardinalDirection, Vec<IncompleteQuadSubMesh>>,
}

impl<'mc, 'chunk, 'neighbor> ChunkMesher<'mc, 'chunk, 'neighbor> {
    fn resolve_model_elements(&mut self, mut textures: HashMap<String, ResourceLocation>, model_location: ResourceLocation) -> ResolvedElements<'mc> {
        let model = self.mcdata.model(model_location);

        #[expect(clippy::iter_over_hash_type, reason = "ordering should not matter -> not 'self' references")]
        for (name, location) in &model.textures {
            match location {
                &Texture::Location(resource_location) | &Texture::Detailed { sprite: resource_location, force_translucent: _ } => {
                    textures.insert(name.clone(), resource_location);
                },
                Texture::Reference(reference) => {
                    if let Some(&val) = textures.get(&reference[1..]) {
                        textures.insert(reference.clone(), val);
                    }
                    else {
                        tracing::warn!(%reference, %model_location, "Could not find a texture reference");
                    }
                },
            }
        }

        match (&model.elements, &model.parent) {
            // If the element array is provided, this overides the parent
            (Some(elements), _) => {
                ResolvedElements {
                    model: model_location,
                    elements,
                    textures,
                }
            },
            (None, &Some(parent)) => {
                self.resolve_model_elements(textures, parent)
            },
            (None, None) => {
                ResolvedElements {
                    model: model_location,
                    elements: &[],
                    textures,
                }
            }
        }
    }

    fn resolve_block_elements(&mut self, block_data: &BlockData) -> ResolvedElements<'mc> {
        let blockstate = self.mcdata.blockstate(block_data.id);

        let model_choice = match blockstate {
            BlockState::Variants { variants } => variants.get(&block_data.state).expect("valid block states"),
            BlockState::Multipart { .. } => panic!("Unsuported multipart blocks"),
        };
        let blockstate_model = match model_choice {
            ModelChoice::Single(model) => model,
            ModelChoice::Multiple(models) => models.first().expect("at leats one model"),
        };
        assert_eq!(blockstate_model.x, 0, "Model rotation not suported");
        assert_eq!(blockstate_model.y, 0, "Model rotation not suported");
        assert_eq!(blockstate_model.z, 0, "Model rotation not suported");
        self.resolve_model_elements(HashMap::new(), blockstate_model.location)
    }

    fn new(
        mcdata: &'mc MinecraftData,
        chunk: &'chunk Chunk,
        neighbors: EnumMap<CardinalDirection, &'neighbor Chunk>,
    ) -> Self {
        let mut this = Self {
            mcdata,
            chunk,
            neighbors,

            block_models: Vec::default(),
            textures: OrderSet::default(),
            quad_submeshes: EnumMap::default(),
        };
        for block_data in chunk.palette() {
            let ResolvedElements { model, elements, textures } = this.resolve_block_elements(block_data);
            let mut full_block_faces = Vec::<FullBlockFace>::new();
            for element in elements {
                if element.rotation.is_some() {
                    tracing::warn!(?model, "Unsuported block model element rotation");
                }
                let from = Vec3::from_array(element.from);
                let to = Vec3::from_array(element.to);

                #[expect(clippy::iter_over_hash_type, reason = "iteration order does not matter")]
                for (&direction, face) in &element.faces {
                    if face.tintindex != -1_i32 {
                        tracing::warn!(?model, "Unsuported tintindex");
                    }
                    let Some(&texture) = textures.get(&face.texture)
                    else { tracing::warn!(?model, texture = face.texture, "Could not get face texture ref"); continue };
                    let axis = direction.axis();
                    if (from - axis) == Vec2::new(0., 0.) && (to - axis) == Vec2::new(16., 16.) {
                        full_block_faces.push(FullBlockFace {
                            direction,
                            texture,
                        });
                    }
                }
            }
            let culling_directions = full_block_faces.iter()
                .map(|face| face.direction)
                .fold(BitFlags::empty(), std::ops::BitOr::bitor);
            this.block_models.push(BlockModel {
                culling_directions,
                full_block_faces: full_block_faces.into_boxed_slice(),
            });
        }
        this
    }

    fn push_face(&mut self, face: CardinalDirection, pos: USizeVec3, texture: ResourceLocation) {
        let submesh = if let Some(submesh) = self.submeshes[face].iter_mut().find(|p| p.texture == texture) {
            submesh
        } else {
            self.submeshes[face].push_mut(IncompleteQuadSubMesh { texture, instances: vec![] })
        };
        submesh.instances.push(create_face_instance_data(pos));
    }

    fn finish(self) -> Box<[QuadSubMesh]> {
        self.submeshes.into_iter()
            .flat_map(|(face, meshes)| meshes.into_iter().map(move |submesh| (face, submesh)))
            .map(|(face, IncompleteQuadSubMesh { texture, instances })| {
                QuadSubMesh {
                    direction: face,
                    texture,
                    instances: instances.into_boxed_slice(),
                }
            })
            .collect()
    }

    fn is_transparent(&self, pos: USizeVec3) -> bool {
        // self.chunk.get(pos).id == location!("minecraft:air")
        todo!()
    }

    fn is_transparent_neighbor(&self, neighbor: CardinalDirection, pos: USizeVec3) -> bool {
        // self.neighbors[neighbor].get(pos).id == location!("minecraft:air")
        todo!()
    }

    fn mesh_for_direction(&mut self, direction: CardinalDirection) {
        for pos in INTERIOR_RANGES[direction] {
            let neighbor = pos + direction;
            if !self.is_transparent(neighbor) { continue }
            // let Some(texture) = self.get_block_texture(self.chunk.get(pos))
            // else { continue };
            // self.push_face(direction, pos, texture);
            todo!()
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
            // let Some(texture) = self.get_block_texture(self.chunk.get(pos))
            // else { continue };
            // self.push_face(direction, pos, texture);
            todo!()
        }
    }
}

pub fn mesh_chunk(mcdata: &MinecraftData, chunk: &Chunk, neighbors: EnumMap<CardinalDirection, &Chunk>) -> ChunkMesh {
    let mut mesher = ChunkMesher::new(mcdata, chunk, neighbors);
    for dir in CardinalDirection::VALUES {
        mesher.mesh_for_direction(dir);
    }
    ChunkMesh { sub_meshes: mesher.finish() }
}
