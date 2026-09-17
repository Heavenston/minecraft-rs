
use std::{cell::RefCell, collections::HashMap, num::Wrapping, rc::Rc, sync::Arc};

use enum_map::{Enum as _, EnumMap};
use glam::{ISizeVec2, ISizeVec3, USizeVec3, Vec2, Vec3};
use ordermap::OrderSet;

use crate::{
    chunk::{BlockData, CHUNK_SIZE, Chunk}, data_extractor::{self, MinecraftData, blockstate::{BlockState, ModelRotation}, model}, resource_location::ResourceLocation, utils::{
        AABB2, AABB3, AxisVec3Ext as _, CardinalDirection, EnumSet, Gather as _, GridAngle, TwentySixDirection, Vec3Range
    },
};

pub mod mesh_shader;

/// This bit field is read from shaders so changes here should be reflected
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

#[bitfield_struct::bitfield(u32, order = Lsb)]
#[derive(bytemuck::NoUninit)]
pub struct SubMeshVertexBits {
    #[bits(2)]
    pub tint_index: u32,
    #[bits(7)]
    pub texture_index: usize,
    #[bits(3)]
    pub face_tint: usize,
    #[bits(20)]
    pub _padding: usize,
}

#[repr(C)]
#[derive(bytemuck::NoUninit, Debug, Clone, Copy)]
pub struct SubMeshVertex {
    pub pos: Vec3,
    pub uv: Vec2,
    // Because of alignment we have a full 32bits remaining
    pub bits: SubMeshVertexBits,
}

pub struct SubMesh {
    pub transparency: ChunkTransparencyMode,
    pub vertices: Box<[SubMeshVertex]>,
}

#[derive(Default)]
struct IncompleteQuadSubMesh {
    instances: Vec<FaceInstanceData>,
}

#[derive(Default)]
struct IncompleteSubMesh {
    pub vertices: Vec<SubMeshVertex>,
}

pub struct ChunkMesh {
    pub quad_submeshes: Box<[QuadSubMesh]>,
    pub submeshes: Box<[SubMesh]>,
    pub mesh_shader: Box<[mesh_shader::Mesh]>,
}

#[derive(Debug, Clone, Copy)]
struct ResolvedTexture {
    force_translucent: bool,
    location: ResourceLocation,
}

struct ResolvedElements<'a> {
    enable_ambient_occlusion: bool,
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
struct FaceTextureData {
    location: ResourceLocation,
    force_translucent: bool,
    tint_index: u32,
}

#[derive(Debug, Clone)]
struct FullBlockFace {
    direction: CardinalDirection,
    uv_flipped: bool,
    uv_rotation: GridAngle,
    texture: FaceTextureData,
}

#[derive(Debug, Clone)]
struct BlockFaceVertex {
    pos: Vec3,
    uv: Vec2,
}

#[derive(Debug, Clone)]
struct BlockFace {
    shade_direction: Option<CardinalDirection>,
    /// Vertices counter clockwise starting from the bottom left.
    vertices: [BlockFaceVertex; 4],
    texture: FaceTextureData,
}

#[derive(Debug, Clone)]
struct ModelFaceList {
    full_faces: Box<[FullBlockFace]>,
    faces: Box<[BlockFace]>,
}

impl ModelFaceList {
    fn is_empty(&self) -> bool {
        self.full_faces.is_empty() && self.faces.is_empty()
    }
}

#[derive(Debug, Clone)]
struct BlockModel {
    enable_ambient_occlusion: bool,
    culling_directions: EnumSet<CardinalDirection>,
    /// Maps the direction each face is *culled* to the list of faces.
    cullable_faces: EnumMap<CardinalDirection, ModelFaceList>,
    faces: ModelFaceList,
}

struct BlockModelResolver {
    mcdata: Arc<MinecraftData>,
    cache: RefCell<HashMap<BlockData, Rc<[BlockModel]>>>,
}

impl BlockModelResolver {
    fn resolve_model_elements_rec(&self, mut textures: HashMap<String, ResolvedTexture>, ambientocclusion: Option<bool>, model_location: ResourceLocation) -> ResolvedElements<'_> {
        let model = self.mcdata.model(model_location);

        #[expect(clippy::iter_over_hash_type, reason = "ordering should not matter -> not 'self' references")]
        for (name, location) in &model.textures {
            match location {
                &model::Texture::Location(location) => {
                    textures.insert(name.clone(), ResolvedTexture {
                        force_translucent: false,
                        location,
                    });
                },
                &model::Texture::Detailed { location, force_translucent } => {
                    textures.insert(name.clone(), ResolvedTexture {
                        force_translucent,
                        location,
                    });
                },
                model::Texture::Reference(reference) => {
                    if let Some(&val) = textures.get(reference.trim_start_matches('#')) {
                        textures.insert(name.clone(), val);
                    }
                    else {
                        tracing::warn!(%reference, %model_location, "Could not find a texture reference");
                    }
                },
            }
        }

        let ambientocclusion = ambientocclusion.or(model.ambientocclusion);
        match (&model.elements, &model.parent) {
            // If the element array is provided, this overides the parent
            (Some(elements), _) => {
                ResolvedElements {
                    enable_ambient_occlusion: ambientocclusion.unwrap_or(true),
                    model: model_location,
                    elements,
                    textures,
                }
            },
            (None, &Some(parent)) => {
                self.resolve_model_elements_rec(textures, ambientocclusion, parent)
            },
            (None, None) => {
                ResolvedElements {
                    enable_ambient_occlusion: ambientocclusion.unwrap_or(true),
                    model: model_location,
                    elements: &[],
                    textures,
                }
            }
        }
    }

    fn resolve_model_elements(&self, model_location: ResourceLocation) -> ResolvedElements<'_> {
        self.resolve_model_elements_rec(HashMap::new(), None, model_location)
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
                    elements: self.resolve_model_elements(blockstate_model.location),
                    model_rotation: blockstate_model.rotation,
                    uvlock: blockstate_model.uvlock,
                    weight: blockstate_model.weight,
                }
            })
            .collect()
    }

    fn resolved_elements_to_block_model(
        ResolvedModelChoice {
            elements: ResolvedElements {
                enable_ambient_occlusion,
                model: model_location, elements, textures,
            },
            model_rotation, uvlock, weight,
        }: ResolvedModelChoice<'_>
    ) -> BlockModel {
        if weight != 1 {
            tracing::warn!("Unsuported non =1 weight");
        }

        #[derive(Default)]
        struct IncompleteFaceList {
            full_faces: Vec<FullBlockFace>,
            faces: Vec<BlockFace>,
        }
        impl From<IncompleteFaceList> for ModelFaceList {
            fn from(val: IncompleteFaceList) -> Self {
                Self {
                    full_faces: val.full_faces.into(),
                    faces: val.faces.into(),
                }
            }
        }

        let mut cullable_faces = EnumMap::<CardinalDirection, IncompleteFaceList>::default();
        let mut faces = IncompleteFaceList::default();
        let mut culling_directions = EnumSet::<CardinalDirection>::empty();
        for &model::Element { from: element_from, to: element_to, rotation: ref element_rotation, shade, shade_direction_override, light_emission, faces: ref model_faces } in elements {
            if element_rotation.is_some() {
                tracing::warn!(?model_location, "Unsuported block model element rotation");
            }
            if light_emission != 0 {
                tracing::warn!(?model_location, light_emission, "Unsuported block model light emission");
            }

            let element_aabb = AABB3 {
                min: Vec3::from_array(element_from),
                max: Vec3::from_array(element_to),
            };

            #[expect(clippy::iter_over_hash_type, reason = "iteration order does not matter")]
            for (&direction, &model::Face {
                ref texture,
                uv,
                cullface,
                rotation: texture_rotation,
                tintindex,
            }) in model_faces {
                let Some(&texture) = textures.get(texture.trim_start_matches('#'))
                else { tracing::warn!(?model_location, texture, ?textures, "Could not get face texture ref"); continue };

                let uv = uv.unwrap_or_else(|| {
                    let [from, to] = element_aabb.face_vertices(direction)
                        .gather([0,2])
                        .map(|v| {
                            let (xs, xp) = if direction == CardinalDirection::NegZ || direction == CardinalDirection::PosX { (-1., 16.) } else { (1., 0.) };
                            let (ys, yp) = if direction != CardinalDirection::PosY { (-1., 16.) } else { (1., 0.) };
                            v.project(direction.axis) * Vec2::new(xs, ys) + Vec2::new(xp, yp)
                        });
                    model::FaceUv { from, to, }
                });
                let (direction, uv_rotation) = model_rotation.rotate_with_uv(direction);
                let cullface = cullface.map(|cullface| model_rotation.rotate_direction(cullface));
                let uv_rotation = if uvlock { GridAngle::Zero } else { uv_rotation } + texture_rotation;
                let axis = direction.axis;

                let can_be_full_face =
                    (element_aabb.min - axis) == Vec2::new(0., 0.) && (element_aabb.max - axis) == Vec2::new(16., 16.) &&
                    shade && shade_direction_override.is_none() &&
                    uv.from.min(uv.to) == Vec2::new(0.,0.) && uv.from.max(uv.to) == Vec2::new(16.,16.) &&
                    element_rotation.is_none()
                ;
                
                let face_texture_data = FaceTextureData {
                    location: texture.location,
                    force_translucent: texture.force_translucent,
                    tint_index: (tintindex + 1i32).try_into().unwrap(),
                };

                let list = cullface.map_or(&mut faces, |cullface| &mut cullable_faces[cullface]);
                if can_be_full_face {
                    culling_directions.insert(direction);
                    list.full_faces.push(FullBlockFace {
                        direction,
                        uv_flipped: uv.from.x > uv.to.x,
                        uv_rotation,
                        texture: face_texture_data,
                    });
                }
                else {
                    if enable_ambient_occlusion {
                        tracing::warn!(?model_location, "Unsuported ambient occlusion on non-full face");
                    }

                    let rotation = element_rotation.as_ref().map(model::ElementRotation::to_affine).unwrap_or_default();
                    let vertices_positions = element_aabb.face_vertices(direction);
                    let vertices_uvs = AABB2 { min: uv.from, max: uv.to }.vertices();

                    list.faces.push(BlockFace {
                        shade_direction: shade.then(|| shade_direction_override.unwrap_or(direction)),
                        vertices: std::array::from_fn(|i| {
                            BlockFaceVertex {
                                pos: rotation.transform_point3(vertices_positions[i]),
                                uv: vertices_uvs[i],
                            }
                        }),
                        texture: face_texture_data,
                    });
                }
            }
        }

        BlockModel {
            enable_ambient_occlusion,
            culling_directions,
            cullable_faces: cullable_faces.map(|_,list| list.into()),
            faces: faces.into(),
        }
    }

    fn resolve_block_model(&self, block_data: &BlockData) -> Rc<[BlockModel]> {
        if let Some(model) = self.cache.borrow().get(block_data) {
            return Rc::clone(model);
        }

        let models = self.resolve_block_elements(block_data)
            .into_iter()
            .map(|elements| Self::resolved_elements_to_block_model(elements))
            .collect();
        Rc::clone(self.cache.borrow_mut().entry(block_data.clone()).insert_entry(models).into_mut())
    }
}

struct ChunkMeshingCtx<'chunk, 'neighbor, 'resolver> {
    config: Config,
    palette_block_models: Box<[Rc<[BlockModel]>]>,
    chunk_pos: ISizeVec3,
    chunk: &'chunk Chunk,
    neighbors: EnumMap<TwentySixDirection, &'neighbor Chunk>,
    model_resolver: &'resolver mut BlockModelResolver,
}

impl ChunkMeshingCtx<'_, '_, '_> {
    fn model_for_pos(&self, pos: USizeVec3) -> &BlockModel {
        let models = &self.palette_block_models[self.chunk.get(pos)];
        if let [model] = &**models {
            model
        }
        else {
            let global_pos = self.chunk_pos * CHUNK_SIZE.as_isizevec3() + pos.as_isizevec3();
            models.get_modulo(get_coordinate_seed(global_pos))
        }
    }

    fn get_delta_signed(&self, delta: ISizeVec3) -> bool {
        let chunk_delta = delta.div_euclid(CHUNK_SIZE.as_isizevec3());
        if chunk_delta == ISizeVec3::ZERO {
            self.model_resolver.resolve_block_model(self.chunk.get_data(delta.as_usizevec3()))[0]
                .culling_directions.is_all()
        }
        else if let Some(&dir) = TwentySixDirection::VALUES.iter().find(|dir| dir.as_isizevec3() == chunk_delta) {
            self.model_resolver.resolve_block_model(self.neighbors[dir].get_data(delta.rem_euclid(CHUNK_SIZE.as_isizevec3()).as_usizevec3()))[0]
                .culling_directions.is_all()
        }
        else {
            unreachable!()
        }
    }

    fn accumulate_ambient_occlusion(&self, direction: CardinalDirection, pos: USizeVec3) -> usize {
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
            // let offset = base_offset.with_direction(direction);

            self.get_delta_signed(pos.as_isizevec3() + offset)
        });

        let mapmap: [[usize; 3];4] = [
            [0,1,2],
            [2,3,4],
            [0,7,6],
            [4,5,6],
        ];

        mapmap.into_iter().map(|[a, b, c]| {
            let a = ambient_occlusion_sides[a];
            let b = ambient_occlusion_sides[b];
            let c = ambient_occlusion_sides[c];
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
struct QuadSubmeshKey(CardinalDirection, ChunkTransparencyMode);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, enum_map::Enum)]
struct SubmeshKey(ChunkTransparencyMode);

struct ChunkMeshBuilder<'a> {
    config: Config,
    mcdata: Arc<MinecraftData>,
    textures: &'a mut OrderSet<ResourceLocation>,
    quad_submeshes: EnumMap<QuadSubmeshKey, IncompleteQuadSubMesh>,
    submeshes: EnumMap<SubmeshKey, IncompleteSubMesh>,
    // For each block, lists faces that are *not* culled
    culled_face_data: Box<[[[EnumSet<CardinalDirection>; CHUNK_SIZE.x]; CHUNK_SIZE.y]; CHUNK_SIZE.z]>,
    mesh_shader: EnumMap<SubmeshKey, mesh_shader::IncompleteMesh>,
}

impl ChunkMeshBuilder<'_> {
    fn push_full_face(&mut self, pos: USizeVec3, face: &FullBlockFace, ambient_occlusion: usize) {
        let texture_data = self.mcdata.texture(face.texture.location);
        let transparency = ChunkTransparencyMode::from_data(texture_data, face.texture.force_translucent);
        let key = QuadSubmeshKey(face.direction, transparency);

        let texture_idx = self.textures.insert_full(face.texture.location).0;
        self.quad_submeshes[key].instances.push(FaceInstanceData::new()
            .with_offset_x(pos.x).with_offset_y(pos.y).with_offset_z(pos.z)
            .with_tint_index(face.texture.tint_index)
            .with_texture_index(texture_idx)
            .with_uv_flipped(face.uv_flipped)
            .with_uv_rotation(face.uv_rotation)
            .with_ambient_occlusion(ambient_occlusion)
        );
    }

    fn push_face(&mut self, pos: USizeVec3, face: &BlockFace, ambient_occlusion: usize) {
        // TODO (a warn is printed during model resolving).
        // No implemented because ambient occlusion with vertices instead of
        // instanced faces involves interpolation i do not want to deal with
        // right now, and i am not sure there is any block model that uses
        // ambient occlusion on non-full faces.
        let _ = ambient_occlusion;

        let texture_data = self.mcdata.texture(face.texture.location);
        let transparency = ChunkTransparencyMode::from_data(texture_data, face.texture.force_translucent);
        let key = SubmeshKey(transparency);

        let texture_idx = self.textures.insert_full(face.texture.location).0;
        let vertices = face.vertices.each_ref().map(|vertex| SubMeshVertex {
            pos: (vertex.pos / 16.) + pos.as_vec3(),
            uv: vertex.uv / 16.,
            bits: SubMeshVertexBits::new()
                .with_tint_index(face.texture.tint_index)
                .with_texture_index(texture_idx)
                .with_face_tint(face.shade_direction.map_or(7, enum_map::Enum::into_usize))
            ,
        });
        self.submeshes[key].vertices.extend([0,1,3,3,1,2].map(|i| vertices[i]));
    }

    fn finish(self) -> ChunkMesh {
        let quad_submeshes = self.quad_submeshes.into_iter()
            .filter(|(_, sub_mesh)| !sub_mesh.instances.is_empty())
            .map(|(QuadSubmeshKey(direction, transparency), IncompleteQuadSubMesh { instances })| {
                QuadSubMesh {
                    direction,
                    transparency,
                    instances: instances.into_boxed_slice(),
                }
            })
            .collect();
        let submeshes = self.submeshes.into_iter()
            .filter(|(_, submesh)| !submesh.vertices.is_empty())
            .map(|(SubmeshKey(transparency), IncompleteSubMesh { vertices })| {
                SubMesh {
                    transparency,
                    vertices: vertices.into_boxed_slice(),
                }
            })
            .collect();
        let mesh_shader = if self.config.enable_mesh_shader {
            self.mesh_shader.into_iter()
                .filter(|(_, submesh)| submesh.data.iter().any(|b| !b.face_mask().is_empty()))
                .map(|(SubmeshKey(transparency), mesh_shader::IncompleteMesh { data, models })| {
                    mesh_shader::Mesh {
                        transparency,
                        data: data.into_boxed_slice(),
                        models: models.into_boxed_slice(),
                    }
                })
                .collect()
        } else {
            Box::default()
        };

        ChunkMesh {
            quad_submeshes,
            submeshes,
            mesh_shader,
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

fn accumulate_culling(ctx: &mut ChunkMeshingCtx, builder: &mut ChunkMeshBuilder) {
    let chunk_global_block_offset = ctx.chunk_pos * CHUNK_SIZE.as_isizevec3();

    for direction in CardinalDirection::VALUES {
        for pos in INTERIOR_RANGES[direction] {
            let model = ctx.model_for_pos(pos);
            let faces = &model.cullable_faces[direction];
            if faces.is_empty() { continue; }

            if ctx.model_for_pos(pos.wrapping_add_signed(direction.as_isizevec3()) /* INTERIOR_RANGE should only return positions for which this does not wrap */).culling_directions.contains(direction.opposit()) { continue }

            builder.culled_face_data[pos.z][pos.y][pos.x].insert(direction);
        }
    }

    for direction in CardinalDirection::VALUES {
        for pos in EXTERIOR_RANGES[direction] {
            let model = ctx.model_for_pos(pos);
            let faces = &model.cullable_faces[direction];
            if faces.is_empty() { continue; }

            let neighbor_block_idx = exterior_neighbor(pos, direction);
            let neighbor_models = ctx.model_resolver.resolve_block_model(ctx.neighbors[direction.into()].get_data(neighbor_block_idx));
            let neighbor_model = if let [neighbor_model] = &*neighbor_models {
                neighbor_model
            } else {
                neighbor_models.get_modulo(get_coordinate_seed(chunk_global_block_offset + pos.as_isizevec3() + direction))
            };
            if neighbor_model.culling_directions.contains(direction.opposit()) { continue }

            builder.culled_face_data[pos.z][pos.y][pos.x].insert(direction);
        }
    }
}

fn mesh_chunk(ctx: &mut ChunkMeshingCtx, builder: &mut ChunkMeshBuilder) {
    for pos in Vec3Range(USizeVec3::ZERO, CHUNK_SIZE) {
        let model = ctx.model_for_pos(pos);

        for direction in CardinalDirection::VALUES {
            let face_list = &model.cullable_faces[direction];
            if face_list.is_empty() || !builder.culled_face_data[pos.z][pos.y][pos.x].contains(direction) { continue }
            let ao = if model.enable_ambient_occlusion { ctx.accumulate_ambient_occlusion(direction, pos) } else { 0 };
            if !ctx.config.enable_mesh_shader {
                for face in &face_list.full_faces {
                    builder.push_full_face(pos, face, ao);
                }
            }
            for face in &face_list.faces {
                builder.push_face(pos, face, ao);
            }
        }

        for face in &model.faces.full_faces {
            builder.push_full_face(pos, face, 0);
        }
        for face in &model.faces.faces {
            builder.push_face(pos, face, 0);
        }
    }
}

fn mesh_chunk_for_mesh_shader(ctx: &mut ChunkMeshingCtx, builder: &mut ChunkMeshBuilder) {
    for (SubmeshKey(transparency), mesh) in &mut builder.mesh_shader {
        struct PaletteIdx {
            offset: usize,
            count: usize,
            faces_for_this_mesh: EnumSet<CardinalDirection>,
        }
        let mut palette_idxs = Vec::<PaletteIdx>::new();

        for block_data in ctx.chunk.palette() {
            let models = ctx.model_resolver.resolve_block_model(block_data);
            let idxs = palette_idxs.push_mut(PaletteIdx {
                offset: mesh.models.len(),
                count: models.len(),
                faces_for_this_mesh: EnumSet::empty(),
            });
            for model in models.iter() {
                let mut face_data: [mesh_shader::FaceDataCombined; 3] = Default::default();
                for (dir, face) in model.cullable_faces.iter().flat_map(|(dir, facelist)| facelist.full_faces.iter().map(move |face| (dir, face))) {
                    if dir != face.direction { tracing::warn!("Full face not culled in its direction??"); continue; }
                    let texture_data = builder.mcdata.texture(face.texture.location);
                    let face_transparency = ChunkTransparencyMode::from_data(texture_data, face.texture.force_translucent);
                    if face_transparency != transparency { continue; }

                    idxs.faces_for_this_mesh.insert(dir);

                    let (texture_idx, _) = builder.textures.insert_full(face.texture.location);

                    let dir_idx = dir.into_usize();
                    let data = mesh_shader::BlockModelFaceData::new()
                        .with_tint_index(face.texture.tint_index)
                        .with_texture_index(texture_idx)
                        // FIXME: Face does not, but should, have a face tint
                        .with_face_tint(dir_idx)
                    ;
                    
                    match dir_idx & 1usize {
                        0 => face_data[dir_idx >> 1usize].set_face1(data),
                        1 => face_data[dir_idx >> 1usize].set_face2(data),
                        _ => unreachable!(),
                    }
                }
                mesh.models.push(mesh_shader::BlockModel {
                    face_data,
                });
            }
            debug_assert_eq!(mesh.models.len(), palette_idxs.last().unwrap().offset + palette_idxs.last().unwrap().count);
        }

        mesh.data.reserve(Vec3Range(USizeVec3::ZERO, CHUNK_SIZE).into_iter().len());
        for z in 0..CHUNK_SIZE.z {
            for gy in (0..CHUNK_SIZE.y).step_by(2) {
                for gx in (0..CHUNK_SIZE.x).step_by(2) {
                    let group_start_idx = mesh.data.len();
                    let mut group_faces = EnumSet::empty();
                    for dy in 0..2usize {
                        for dx in 0..2usize {
                            let pos = USizeVec3::new(gx + dx, gy + dy, z);

                            let palette_idx = ctx.chunk.get(pos);
                            let idx = &palette_idxs[palette_idx];
                            let variant_offset = if idx.count == 0 {
                                0
                            } else {
                                let global_pos = ctx.chunk_pos * CHUNK_SIZE.as_isizevec3() + pos.as_isizevec3();
                                usize::try_from(get_coordinate_seed(global_pos) % u64::try_from(idx.count).unwrap()).unwrap()
                            };
                            let faces = builder.culled_face_data[pos.z][pos.y][pos.x].intersection(idx.faces_for_this_mesh);
                            group_faces = group_faces.union(faces);
                            mesh.data.push(mesh_shader::Block::new()
                                .with_model_idx(idx.offset + variant_offset)
                                .with_face_mask(faces)
                            );
                        }
                    }
                    for i in &mut mesh.data[group_start_idx..] {
                        i.set_group_face_mask(group_faces);
                    }
                }
            }
        }
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub struct Config {
    pub enable_mesh_shader: bool,
}

pub struct ChunkMesher {
    config: Config,
    resolver: BlockModelResolver,
    textures: OrderSet<ResourceLocation>,
}

impl ChunkMesher {
    pub fn new(config: Config, mcdata: Arc<MinecraftData>) -> Self {
        Self {
            config,
            resolver: BlockModelResolver {
                mcdata,
                cache: Default::default(),
            },
            textures: OrderSet::new(),
        }
    }

    pub fn textures(&self) -> impl ExactSizeIterator<Item = ResourceLocation> {
        self.textures.iter().copied()
    }

    pub fn mesh_chunk(&mut self, chunk_pos: ISizeVec3, chunk: &Chunk, neighbors: EnumMap<TwentySixDirection, &Chunk>) -> ChunkMesh {
        let palette_block_models: Box<[Rc<[BlockModel]>]> = chunk.palette().iter()
            .map(|block_data| self.resolver.resolve_block_model(block_data))
            .collect();

        let mut builder = ChunkMeshBuilder {
            config: self.config,
            mcdata: Arc::clone(&self.resolver.mcdata),
            textures: &mut self.textures,
            quad_submeshes: EnumMap::default(),
            submeshes: EnumMap::default(),
            culled_face_data: Box::default(),
            mesh_shader: EnumMap::default(),
        };
        let mut ctx = ChunkMeshingCtx {
            config: self.config,
            palette_block_models,
            chunk_pos,
            chunk,
            neighbors,
            model_resolver: &mut self.resolver,
        };
        accumulate_culling(&mut ctx, &mut builder);
        mesh_chunk(&mut ctx, &mut builder);
        if self.config.enable_mesh_shader {
            mesh_chunk_for_mesh_shader(&mut ctx, &mut builder);
        }
        builder.finish()
    }
}
