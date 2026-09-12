
use std::{collections::HashMap, num::Wrapping, sync::Arc};

use enum_map::EnumMap;
use glam::{ISizeVec2, ISizeVec3, USizeVec3, Vec2, Vec3};
use ordermap::OrderSet;
use static_assertions as ca;

use crate::{chunk::{BlockData, CHUNK_SIZE, Chunk}, data_extractor::{self, MinecraftData, blockstate::{BlockState, ModelRotation}, model::{self, Texture}}, resource_location::ResourceLocation, utils::{CardinalDirection, EnumSet, GridAngle, Vec3Range}};

ca::const_assert!(CHUNK_SIZE.x.is_power_of_two());
ca::const_assert!(CHUNK_SIZE.y.is_power_of_two());
ca::const_assert!(CHUNK_SIZE.z.is_power_of_two());
const CHUNK_OFFSET_BITS: u32 = CHUNK_SIZE.x.ilog2() + CHUNK_SIZE.y.ilog2() + CHUNK_SIZE.z.ilog2();
ca::const_assert!(CHUNK_OFFSET_BITS == 12);

#[bitfield_struct::bitfield(u32, order = Lsb)]
#[derive(bytemuck::NoUninit)]
pub struct FaceInstanceData {
    #[bits(4)]
    pub offset_x: usize,
    #[bits(4)]
    pub offset_y: usize,
    #[bits(4)]
    pub offset_z: usize,
    #[bits(2)]
    pub tint_index: u32,
    #[bits(7)]
    pub texture_index: usize,
    #[bits(2)]
    pub uv_rotation: GridAngle,
    pub uv_flipped: bool,
    #[bits(8)]
    pub ambient_occlusion: usize,
}

const fn create_interior_ranges() -> EnumMap<CardinalDirection, Vec3Range> {
    const fn range_for_direction(direction: CardinalDirection) -> Vec3Range {
        let mut interior_min = USizeVec3::ZERO;
        let mut interior_max = CHUNK_SIZE;
        if direction.sign.is_positive() {
            interior_max[direction.axis] -= 1;
        }
        else {
            interior_min[direction.axis] += 1;
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
        if direction.sign.is_positive() {
            interior_min[direction.axis] = interior_max[direction.axis] - 1;
        }
        else {
            interior_max[direction.axis] = interior_min[direction.axis] + 1;
        }
        Vec3Range(interior_min, interior_max)
    }
    EnumMap::from_array(CardinalDirection::VALUES.map(range_for_direction))
}
static EXTERIOR_RANGES: EnumMap<CardinalDirection, Vec3Range> = create_exterior_ranges();

#[test]
fn test_interior_range() {
    for (dir, &range) in &INTERIOR_RANGES {
        for val in range {
            std::assert_matches!(val.checked_add_signed(dir.as_isizevec3()), Some(_));
        }
    }
}

#[test]
fn test_exterior_range() {
    for (dir, &range) in &EXTERIOR_RANGES {
        for val in range {
            if dir.sign.is_positive() {
                assert_eq!(val[dir.axis], CHUNK_SIZE[dir.axis] - 1);
            }
            else {
                assert_eq!(val[dir.axis], 0);
            }
        }
    }
}

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
    culled: EnumSet<CardinalDirection>,
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

struct ResolvedModelChoice<'a> {
    elements: ResolvedElements<'a>,
    model_rotation: ModelRotation,
    uvlock: bool,
    weight: u32,
}

#[derive(Debug, Clone)]
struct FullBlockFace {
    texture: ResourceLocation,
    force_translucent: bool,
    tint_index: u32,
    uv_flipped: bool,
    uv_rotation: GridAngle,
}

#[derive(Debug, Clone)]
struct BlockModel {
    culling_directions: EnumSet<CardinalDirection>,
    full_block_faces: EnumMap<CardinalDirection, Box<[FullBlockFace]>>,
}

struct BlockModelResolver {
    mcdata: Arc<MinecraftData>,
    cache: HashMap<BlockData, Arc<[BlockModel]>>,
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

    fn resolve_block_elements(&self, block_data: &BlockData) -> Box<[ResolvedModelChoice<'_>]> {
        let blockstate = self.mcdata.blockstate(block_data.id);

        let model_choice = match blockstate {
            BlockState::Variants { variants } => variants.get(&block_data.state).expect("valid block states"),
            BlockState::Multipart { .. } => panic!("Unsuported multipart blocks"),
        };
        model_choice
            .as_slice()
            .iter()
            .map(|blockstate_model| {
                ResolvedModelChoice {
                    elements: self.resolve_model_elements(HashMap::new(), blockstate_model.location),
                    model_rotation: blockstate_model.rotation,
                    uvlock: blockstate_model.uvlock,
                    weight: blockstate_model.weight,
                }
            })
            .collect()
    }

    fn resolved_elements_to_block_model(
        &self,
        ResolvedModelChoice {
            elements: ResolvedElements {
                model, elements, textures,
            },
            model_rotation, uvlock, weight,
        }: ResolvedModelChoice<'_>
    ) -> BlockModel {
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

                let uv = face.uv.unwrap_or_else(|| {
                    model::FaceUv { from: from - direction.axis, to: to - direction.axis }
                });
                let (direction, uv_rotation) = model_rotation.rotate_with_uv(direction);
                let uv_rotation = if uvlock { GridAngle::Zero } else { uv_rotation } + face.rotation;
                let axis = direction.axis;

                let uv_full = if uv == model::FaceUv::FULL_FACE {
                    Some(false)
                } else if uv.swap_x() == model::FaceUv::FULL_FACE {
                    Some(true)
                } else {
                    None
                };
                
                if (from - axis) == Vec2::new(0., 0.) && (to - axis) == Vec2::new(16., 16.) && let Some(uv_flipped) = uv_full {
                    full_block_faces[direction].push(FullBlockFace {
                        texture: texture.location,
                        force_translucent: texture.force_translucent,
                        tint_index: (face.tintindex + 1i32).try_into().unwrap(),
                        uv_flipped,
                        uv_rotation,
                    });
                }
                else {
                    tracing::warn!(?face, ?direction, ?uv, "Unsuported non-full face");
                }
            }
        }
        let culling_directions = full_block_faces.iter()
            .filter(|(_, faces)| faces.iter().any(|face| !face.force_translucent && self.mcdata.texture(face.texture).is_opaque()))
            .map(|(dir,_)| dir)
            .collect::<EnumSet<CardinalDirection>>();

        if weight != 1 {
            tracing::warn!("Unsuported non =1 weight");
        }

        BlockModel {
            culling_directions,
            full_block_faces: full_block_faces.map(|_, vec| vec.into_boxed_slice()),
        }
    }

    fn resolve_block_model(&mut self, block_data: &BlockData) -> &Arc<[BlockModel]> {
        if let Some(model) = self.cache.get(block_data) {
            return model;
        }

        let models = self.resolve_block_elements(block_data)
            .into_iter()
            .map(|elements| self.resolved_elements_to_block_model(elements))
            .collect();
        self.cache.entry(block_data.clone()).insert_entry(models).into_mut()
    }
}

struct ChunkMeshingCtx<'chunk, 'neighbor, 'resolver> {
    chunk_pos: ISizeVec3,
    chunk: &'chunk Chunk,
    neighbors: EnumMap<CardinalDirection, &'neighbor Chunk>,
    model_resolver: &'resolver mut BlockModelResolver,
}

impl ChunkMeshingCtx<'_, '_, '_> {
    fn get_delta_signed(&mut self, delta: ISizeVec3) -> bool {
        let chunk_delta = delta.div_euclid(CHUNK_SIZE.as_isizevec3());
        if chunk_delta == ISizeVec3::ZERO {
            self.model_resolver.resolve_block_model(self.chunk.get_data(delta.as_usizevec3()))[0]
                .culling_directions.is_all()
        }
        else if let Some(&dir) = CardinalDirection::VALUES.iter().find(|dir| dir.as_isizevec3() == chunk_delta) {
            self.model_resolver.resolve_block_model(self.neighbors[dir].get_data(delta.rem_euclid(CHUNK_SIZE.as_isizevec3()).as_usizevec3()))[0]
                .culling_directions.is_all()
        }
        else {
            // TODO
            false
        }
    }

    fn accumulate_ao(&mut self, direction: CardinalDirection, pos: USizeVec3) -> usize {
        let ambient_occlusion_sides: [bool; 8] = [
            ISizeVec2::new(-1,  0),
            ISizeVec2::new(-1, -1),
            ISizeVec2::new( 0, -1),
            ISizeVec2::new( 1, -1),
            ISizeVec2::new( 1,  0),
            ISizeVec2::new( 1,  1),
            ISizeVec2::new( 0,  1),
            ISizeVec2::new(-1,  1),
        ].map(|base_offset| {
            let ISizeVec2 { x: u, y: v } = base_offset;
            let offset = match direction {
                CardinalDirection::PosX => ISizeVec3::new( 1,  v, -u),
                CardinalDirection::NegX => ISizeVec3::new(-1,  v,  u),
                CardinalDirection::PosY => ISizeVec3::new( u,  1, -v),
                CardinalDirection::NegY => ISizeVec3::new( u, -1,  v),
                CardinalDirection::PosZ => ISizeVec3::new( u,  v,  1),
                CardinalDirection::NegZ => ISizeVec3::new(-u,  v, -1),
            };

            self.get_delta_signed(pos.as_isizevec3() + offset)
        });

        (0..3usize).map(|vertex_idx| {
            let offset = vertex_idx * 2;
            let a = ambient_occlusion_sides[offset];
            let b = ambient_occlusion_sides[offset+1];
            let c = ambient_occlusion_sides[(offset+2)%8];
            if a && c {
                3
            } else {
                usize::from(a || c) + usize::from(b)
            }
        }).rfold(0usize, |acc, val| (acc << 2usize) | val)
    }
}

/// Should be similar to how minecraft does it.
/// Though they do not use this value the same way.
fn get_coordinate_seed(pos: ISizeVec3) -> u64 {
    let pos = pos.as_i64vec3();
    let l = (Wrapping(pos.x) * Wrapping(3_129_871_i64)) ^ (Wrapping(pos.z) * Wrapping(116_129_781_i64)) ^ Wrapping(pos.y);
    let l = l * l * Wrapping(42_317_861_i64) + l * Wrapping(11i64);
    (Wrapping(l.0.cast_unsigned()) >> 16).0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, enum_map::Enum)]
struct DirAndTransparency(CardinalDirection, ChunkTransparencyMode);

struct ChunkMeshBuilder<'a> {
    mcdata: Arc<MinecraftData>,
    textures: &'a mut OrderSet<ResourceLocation>,
    quad_submeshes: EnumMap<DirAndTransparency, IncompleteQuadSubMesh>,
}

impl ChunkMeshBuilder<'_> {
    fn push_face(&mut self, dir: CardinalDirection, pos: USizeVec3, face: &FullBlockFace, ambient_occlusion: usize) {
        let texture_data = self.mcdata.texture(face.texture);
        let transparency = ChunkTransparencyMode::from_data(texture_data, face.force_translucent);
        let key = DirAndTransparency(dir, transparency);

        let texture_idx = self.textures.insert_full(face.texture).0;
        self.quad_submeshes[key].instances.push(FaceInstanceData::new()
            .with_offset_x(pos.x).with_offset_y(pos.y).with_offset_z(pos.z)
            .with_tint_index(face.tint_index)
            .with_texture_index(texture_idx)
            .with_uv_flipped(face.uv_flipped)
            .with_uv_rotation(face.uv_rotation)
            .with_ambient_occlusion(ambient_occlusion)
        );
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
    if direction.sign.is_positive() {
        neighbor[direction.axis] = 0;
    }
    else {
        neighbor[direction.axis] = CHUNK_SIZE[direction.axis]-1;
    }
    neighbor
}

trait SliceExt<T> {
    fn get_modulo(&self, idx: u64) -> &T;
}

impl<T> SliceExt<T> for [T] {
    fn get_modulo(&self, idx: u64) -> &T {
        &self[usize::try_from(idx % u64::try_from(self.len()).unwrap()).unwrap()]
    }
}

fn mesh_chunk(ctx: &mut ChunkMeshingCtx, builder: &mut ChunkMeshBuilder) {
    let chunk_offset = ctx.chunk_pos * CHUNK_SIZE.as_isizevec3();

    let block_models: Box<[Arc<[BlockModel]>]> = ctx.chunk.palette().iter()
        .map(|block_data| Arc::clone(ctx.model_resolver.resolve_block_model(block_data)))
        .collect();

    let model_for_pos = |pos: USizeVec3| -> &BlockModel {
        let models = &block_models[ctx.chunk.get(pos)];
        if let [model] = &**models {
            model
        }
        else {
            let global_pos = chunk_offset + pos.as_isizevec3();
            models.get_modulo(get_coordinate_seed(global_pos))
        }
    };


    for direction in CardinalDirection::VALUES {
        for pos in INTERIOR_RANGES[direction] {
            let faces = &model_for_pos(pos).full_block_faces[direction];
            if faces.is_empty() { continue; }
            if model_for_pos(pos.checked_add_signed(direction.as_isizevec3()).expect("INTERIOR_RANGE should only return positions for which this works")).culling_directions.contains(direction.opposit()) { continue }
            for face in faces {
                let ao = ctx.accumulate_ao(direction, pos);
                builder.push_face(direction, pos, face, ao);
            }
        }
    }

    for direction in CardinalDirection::VALUES {
        for pos in EXTERIOR_RANGES[direction] {
            let faces = &model_for_pos(pos).full_block_faces[direction];
            if faces.is_empty() { continue; }
            let neighbor_block_idx = exterior_neighbor(pos, direction);
            let neighbor_models = ctx.model_resolver.resolve_block_model(ctx.neighbors[direction].get_data(neighbor_block_idx));
            let neighbor_model = if let [neighbor_model] = &**neighbor_models {
                neighbor_model
            } else {
                neighbor_models.get_modulo(get_coordinate_seed(chunk_offset + pos.as_isizevec3() + direction))
            };
            if neighbor_model.culling_directions.contains(direction.opposit()) { continue }
            for face in faces {
                let ao = ctx.accumulate_ao(direction, pos);
                builder.push_face(direction, pos, face, ao);
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

    pub fn mesh_chunk(&mut self, chunk_pos: ISizeVec3, chunk: &Chunk, neighbors: EnumMap<CardinalDirection, &Chunk>) -> ChunkMesh {
        let mut builder = ChunkMeshBuilder {
            mcdata: Arc::clone(&self.resolver.mcdata),
            quad_submeshes: EnumMap::default(),
            textures: &mut self.textures,
        };
        let mut ctx = ChunkMeshingCtx {
            chunk_pos,
            chunk,
            neighbors,
            model_resolver: &mut self.resolver,
        };
        mesh_chunk(&mut ctx, &mut builder);
        builder.finish()
    }
}
