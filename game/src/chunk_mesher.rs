
use std::{collections::HashMap, sync::Arc};

use enum_map::EnumMap;
use enumflags2::BitFlags;
use glam::{USizeVec3, Vec2, Vec3};
use ordermap::OrderSet;
use static_assertions as ca;

use crate::{chunk::{BlockData, CHUNK_SIZE, Chunk}, data_extractor::{self, MinecraftData, blockstate::{BlockState, ModelChoice}, model::{self, Texture}}, resource_location::ResourceLocation, utils::{CardinalDirection, Vec3Range}};

ca::const_assert!(CHUNK_SIZE.x.is_power_of_two());
ca::const_assert!(CHUNK_SIZE.y.is_power_of_two());
ca::const_assert!(CHUNK_SIZE.z.is_power_of_two());
const CHUNK_OFFSET_BITS: u32 = CHUNK_SIZE.x.ilog2() + CHUNK_SIZE.y.ilog2() + CHUNK_SIZE.z.ilog2();
ca::const_assert!(CHUNK_OFFSET_BITS == 12);
const TEXTURE_INDEX_BITS: u32 = 8;
const MAX_TEXTURE_COUNT: u32 = 2u32.pow(TEXTURE_INDEX_BITS);
const MAX_TEXTURE_SIZE: usize = MAX_TEXTURE_COUNT as usize;

type FaceInstanceData = u32;

fn create_face_instance_data(offset: USizeVec3, tint_index: u8, texture_index: u32) -> FaceInstanceData {
    debug_assert_eq!(offset.as_uvec3().as_usizevec3(), offset);
    debug_assert_eq!(tint_index & 0xF, tint_index);
    debug_assert_eq!(texture_index & (MAX_TEXTURE_COUNT - 1), texture_index);
    let offset = offset.as_uvec3();
    (((((texture_index << 8 | u32::from(tint_index)) << CHUNK_SIZE.x.ilog2()) | offset.x) << CHUNK_SIZE.y.ilog2()) | offset.y) << CHUNK_SIZE.z.ilog2() | offset.z
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, enum_map::Enum)]
pub enum ChunkTransparencyMode {
    Opaque,
    Cutout,
    Translucent,
}

impl ChunkTransparencyMode {
    fn from_data(data: &data_extractor::TextureInfo, force_translucent: bool) -> Self {
        if force_translucent || data.has_translucent {
            Self::Translucent
        }
        else if data.has_transparent {
            Self::Cutout
        } else {
            Self::Opaque
        }
    }
}

/// Sub mesh for a chunk that only contains full block faces.
pub struct QuadSubMesh {
    pub direction: CardinalDirection,
    pub transparency: ChunkTransparencyMode,
    pub instances: Box<[FaceInstanceData]>,
}

#[expect(dead_code, reason = "todo")]
pub struct FullVertex {
    pub pos: Vec3,
    pub uv: Vec2,
    pub texture: u32,
}

#[expect(dead_code, reason = "todo")]
pub struct SubMesh {
    pub vertices: Vec<FullVertex>,
}

#[derive(Default)]
struct IncompleteQuadSubMesh {
    instances: Vec<FaceInstanceData>,
}

#[expect(dead_code, reason = "todo")]
struct IncompleteSubMeshInstance {
    model: usize,
    culled: BitFlags<CardinalDirection>,
}

#[expect(dead_code, reason = "todo")]
struct IncompleteSubMesh {
    models: Vec<usize>,
    instances: Vec<IncompleteSubMeshInstance>,
}

pub struct ChunkMesh {
    pub quad_submeshes: Box<[QuadSubMesh]>,
    #[expect(dead_code, reason = "todo")]
    pub submeshes: Box<[SubMesh]>,
}

#[derive(Debug, Clone, Copy)]
struct ResolvedTexture {
    force_translucent: bool,
    location: ResourceLocation,
}

struct ResolvedElements<'a> {
    model: ResourceLocation,
    elements: &'a [model::Element],
    textures: HashMap<String, ResolvedTexture>,
}

#[derive(Debug, Clone)]
struct FullBlockFace {
    texture: ResourceLocation,
    force_translucent: bool,
    tint_index: u8,
}

#[derive(Debug, Clone)]
struct BlockModel {
    culling_directions: BitFlags<CardinalDirection>,
    full_block_faces: EnumMap<CardinalDirection, Box<[FullBlockFace]>>,
}

struct BlockModelResolver {
    mcdata: Arc<MinecraftData>,
    cache: HashMap<BlockData, BlockModel>,
}

impl BlockModelResolver {
    fn resolve_model_elements(&self, mut textures: HashMap<String, ResolvedTexture>, model_location: ResourceLocation) -> ResolvedElements<'_> {
        let model = self.mcdata.model(model_location);

        #[expect(clippy::iter_over_hash_type, reason = "ordering should not matter -> not 'self' references")]
        for (name, location) in &model.textures {
            match location {
                &Texture::Location(location) => {
                    textures.insert(name.clone(), ResolvedTexture {
                        force_translucent: false,
                        location,
                    });
                },
                &Texture::Detailed { location, force_translucent } => {
                    textures.insert(name.clone(), ResolvedTexture {
                        force_translucent,
                        location,
                    });
                },
                Texture::Reference(reference) => {
                    if let Some(&val) = textures.get(reference.trim_start_matches('#')) {
                        textures.insert(name.clone(), val);
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

    fn resolve_block_elements(&self, block_data: &BlockData) -> ResolvedElements<'_> {
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

    fn resolve_block_model(&mut self, block_data: &BlockData) -> &BlockModel {
        if let Some(model) = self.cache.get(block_data) {
            return model;
        }

        let ResolvedElements { model, elements, textures } = self.resolve_block_elements(block_data);
        let mut full_block_faces = EnumMap::<CardinalDirection, Vec<FullBlockFace>>::default();
        for element in elements {
            if element.rotation.is_some() {
                tracing::warn!(?model, "Unsuported block model element rotation");
            }
            let from = Vec3::from_array(element.from);
            let to = Vec3::from_array(element.to);

            #[expect(clippy::iter_over_hash_type, reason = "iteration order does not matter")]
            for (&direction, face) in &element.faces {
                let Some(&texture) = textures.get(face.texture.trim_start_matches('#'))
                else { tracing::warn!(?model, texture = face.texture, ?textures, "Could not get face texture ref"); continue };
                let axis = direction.axis();
                if (from - axis) == Vec2::new(0., 0.) && (to - axis) == Vec2::new(16., 16.) {
                    full_block_faces[direction].push(FullBlockFace {
                        texture: texture.location,
                        force_translucent: texture.force_translucent,
                        tint_index: (face.tintindex + 1i32).try_into().unwrap(),
                    });
                }
            }
        }
        let culling_directions = full_block_faces.iter()
            .filter(|(_, faces)| faces.iter().any(|face| !face.force_translucent && self.mcdata.texture(face.texture).is_opaque()))
            .map(|(dir,_)| dir)
            .fold(BitFlags::empty(), std::ops::BitOr::bitor);
        self.cache.entry(block_data.clone()).insert_entry(BlockModel {
            culling_directions,
            full_block_faces: full_block_faces.map(|_, vec| vec.into_boxed_slice()),
        }).into_mut()
    }
}

struct ChunkMeshingCtx<'chunk, 'neighbor, 'resolver> {
    chunk: &'chunk Chunk,
    neighbors: EnumMap<CardinalDirection, &'neighbor Chunk>,
    model_resolver: &'resolver mut BlockModelResolver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, enum_map::Enum)]
struct DirAndTransparency(CardinalDirection, ChunkTransparencyMode);

struct ChunkMeshBuilder<'a> {
    mcdata: Arc<MinecraftData>,
    textures: &'a mut OrderSet<ResourceLocation>,
    quad_submeshes: EnumMap<DirAndTransparency, IncompleteQuadSubMesh>,
}

impl ChunkMeshBuilder<'_> {
    fn push_face(&mut self, dir: CardinalDirection, pos: USizeVec3, face: &FullBlockFace) {
        let texture_data = self.mcdata.texture(face.texture);
        let transparency = ChunkTransparencyMode::from_data(texture_data, face.force_translucent);
        let key = DirAndTransparency(dir, transparency);

        let texture_idx = self.textures.insert_full(face.texture).0;
        assert!(texture_idx < MAX_TEXTURE_SIZE);
        self.quad_submeshes[key].instances.push(create_face_instance_data(pos, face.tint_index, texture_idx.try_into().unwrap()));
    }

    fn finish(self) -> ChunkMesh {
        let quad_submeshes = self.quad_submeshes.into_iter()
            .filter(|(_, sub_mesh)| !sub_mesh.instances.is_empty())
            .map(|(DirAndTransparency(direction, transparency), IncompleteQuadSubMesh { instances })| {
                QuadSubMesh {
                    direction,
                    transparency,
                    instances: instances.into_boxed_slice(),
                }
            })
            .collect();

        ChunkMesh {
            quad_submeshes,
            submeshes: Box::default(),
        }
    }
}

fn exterior_neighbor(pos: USizeVec3, direction: CardinalDirection) -> USizeVec3 {
    let mut neighbor = pos;
    if direction.is_positive() {
        neighbor[direction.axis()] = 0;
    }
    else {
        neighbor[direction.axis()] = CHUNK_SIZE[direction.axis()]-1;
    }
    neighbor
}

fn mesh_chunk(ctx: &mut ChunkMeshingCtx, builder: &mut ChunkMeshBuilder) {
    let block_models: Box<[BlockModel]> = ctx.chunk.palette().iter()
        .map(|block_data| ctx.model_resolver.resolve_block_model(block_data).clone())
        .collect();

    for direction in CardinalDirection::VALUES {
        for pos in INTERIOR_RANGES[direction] {
            let palette_idx = ctx.chunk.get(pos);
            let faces = &block_models[palette_idx].full_block_faces[direction];
            if faces.is_empty() { continue; }
            if block_models[ctx.chunk.get(pos + direction)].culling_directions.contains(direction.opposit()) { continue }
            for face in faces {
                builder.push_face(direction, pos, face);
            }
        }
    }

    for direction in CardinalDirection::VALUES {
        for pos in EXTERIOR_RANGES[direction] {
            let palette_idx = ctx.chunk.get(pos);
            let faces = &block_models[palette_idx].full_block_faces[direction];
            if faces.is_empty() { continue; }
            let neighbor_block_idx = exterior_neighbor(pos, direction);
            let neighbor_model = ctx.model_resolver.resolve_block_model(ctx.neighbors[direction].get_data(neighbor_block_idx));
            if neighbor_model.culling_directions.contains(direction.opposit()) { continue }
            for face in faces {
                builder.push_face(direction, pos, face);
            }
        }
    }
}

pub struct ChunkMesher {
    resolver: BlockModelResolver,
    textures: OrderSet<ResourceLocation>,
}

impl ChunkMesher {
    pub fn new(mcdata: Arc<MinecraftData>) -> Self {
        Self {
            resolver: BlockModelResolver {
                mcdata,
                cache: HashMap::default(),
            },
            textures: OrderSet::new(),
        }
    }

    pub fn textures(&self) -> impl ExactSizeIterator<Item = ResourceLocation> {
        self.textures.iter().copied()
    }

    pub fn mesh_chunk(&mut self, chunk: &Chunk, neighbors: EnumMap<CardinalDirection, &Chunk>) -> ChunkMesh {
        let mut builder = ChunkMeshBuilder {
            mcdata: Arc::clone(&self.resolver.mcdata),
            quad_submeshes: EnumMap::default(),
            textures: &mut self.textures,
        };
        let mut ctx = ChunkMeshingCtx {
            chunk,
            neighbors,
            model_resolver: &mut self.resolver,
        };
        mesh_chunk(&mut ctx, &mut builder);
        builder.finish()
    }
}
