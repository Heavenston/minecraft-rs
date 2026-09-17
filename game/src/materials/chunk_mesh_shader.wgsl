enable wgpu_mesh_shader;

const pi = radians(180.0);
const tau = radians(360.0);

override ENABLE_CUTOUT: bool;
override AMBIENT_OCCLUSION_DEBUG: bool = false;

//  /-----+
//  | 2   3
//  | 
//  + 0   1
fn get_cube_uv(vertex_index: u32) -> vec2f {
    return vec2f(
        f32(vertex_index & 1u),
        f32(1u - (vertex_index >> 1u)),
    );
}

fn get_cube_vertex(uv: vec2f, dir: u32) -> vec3f {
    let u = uv.x;
    let v = 1.0 - uv.y;
//  v 2   3
//  | 
//  | 0   1
//  \-----u
    switch (dir) {
        case 0u: { return vec3f(1.0    , v  , 1.0 - u); } // +X
        case 1u: { return vec3f(0.0    , v  ,       u); } // -X
        case 2u: { return vec3f(u      , 1.0, 1.0 - v); } // +Y
        case 3u: { return vec3f(u      , 0.0,       v); } // -Y
        case 4u: { return vec3f(u      , v  , 1.0    ); } // +Z
        default: { return vec3f(1.0 - u, v  , 0.0    ); } // -Z
    }
}
fn get_cube_normal(dir: u32) -> vec3f {
    switch (dir) {
        case 0u: { return vec3f(1.0, 0.0, 0.0); }    // +X
        case 1u: { return vec3f(-1.0, 0.0, 0.0); }   // -X
        case 2u: { return vec3f(0.0, 1.0, 0.0); }    // +Y
        case 3u: { return vec3f(0.0, -1.0, 0.0); }   // -Y
        case 4u: { return vec3f(0.0, 0.0, 1.0); }    // +Z
        default: { return vec3f(0.0, 0.0, -1.0); }   // -Z
    }
}

struct WorldUniform {
    view_projection_matrix: mat4x4f,
    time: f32,
};

struct BlockModelFaceData {
    tint_index: u32,
    texture_index: u32,
    face_tint: u32,
};

struct BlockModel {
    // Starting with lowest significant bit
    //   tint_index    (2 bits)
    //   texture_index (7 bits)
    //   face_tint     (3 bits)
    //  x2 for two faces per integer
    faces_data: array<u32, 3>,
};

@group(0) @binding(0) var<uniform> world: WorldUniform;
@group(1) @binding(0) var texture_sampler: sampler;
@group(1) @binding(1) var texture: texture_2d_array<f32>;
@group(2) @binding(0) var<storage, read> chunk_data: array<u32>;
@group(2) @binding(1) var<storage, read> models: array<BlockModel>;

struct Immediates {
    chunk_offset: vec3f,
};

var<immediate> imm: Immediates;

fn block_info_at(pos: vec3i) -> u32 {
    let idx = pos.x + pos.y * 16 + pos.z * 16 * 16;
    return chunk_data[idx];
}

fn block_info_model(block_info: u32) -> u32 {
    return block_info & 0xFFFF;
}

fn block_info_has_any_face(block_info: u32) -> bool {
    return (block_info & 0x3F0000u) != 0;
}

fn block_info_has_face(block_info: u32, face_dir: u32) -> bool {
    return (block_info & (1u << (16 + face_dir))) != 0;
}

fn block_model_get_face_data(model: u32, face_dir: u32) -> BlockModelFaceData {
    let packed = models[model].faces_data[face_dir >> 1];
    let offset = (face_dir & 0x1) * 12;

    var out: BlockModelFaceData;
    out.tint_index    = (packed >> (offset + 0u)) & 0x03u;
    out.texture_index = (packed >> (offset + 2u)) & 0x7Fu;
    out.face_tint     = (packed >> (offset + 9u)) & 0x07u;
    return out;
}

// struct VertexOutput {
//     @builtin(position) position: vec4f,
//     @location(0) texcoord: vec2f,
//     @interpolate(flat) @location(1) tint_index: u32,
//     @interpolate(flat) @location(2) texture_index: u32,
//     @interpolate(flat) @location(3) face_tint: u32,
// };

// @vertex fn vs(
//     @location(0) vertex_pos: vec3f,
//     @location(1) vertex_uv: vec2f,
//     @location(2) bits_data: u32,
// ) -> VertexOutput {
//     var output: VertexOutput;
//     output.position = world.view_projection_matrix * vec4f(vertex_pos + imm.chunk_offset, 1.0);
//     output.texcoord = vertex_uv;
//     output.tint_index = (bits_data >> 0) & 0x3;
//     output.texture_index = (bits_data >> 2) & 0x7f;
//     output.face_tint = (bits_data >> 9) & 0x7;
//     return output;
// }

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) texcoord: vec2f,
}

struct PrimitiveOutput {
    @builtin(triangle_indices) indices: vec3<u32>,
    @builtin(cull_primitive) cull: bool,
    @interpolate(flat, either) @per_primitive @location(1) tint_index: u32,
    @interpolate(flat, either) @per_primitive @location(2) texture_index: u32,
    @interpolate(flat, either) @per_primitive @location(3) face_tint: u32,
}
struct PrimitiveInput {
    @interpolate(flat, either) @per_primitive @location(1) tint_index: u32,
    @interpolate(flat, either) @per_primitive @location(2) texture_index: u32,
    @interpolate(flat, either) @per_primitive @location(3) face_tint: u32,
}

struct MeshOutput {
    @builtin(vertices) vertices: array<VertexOutput, 24>,
    @builtin(primitives) primitives: array<PrimitiveOutput, 12>,
    @builtin(vertex_count) vertex_count: u32,
    @builtin(primitive_count) primitive_count: u32,
}
var<workgroup> mesh_output: MeshOutput;

fn run_mesh_shader(block_pos: vec3i, dir: u32) {
    let block_info = block_info_at(block_pos);
    if !block_info_has_any_face(block_info) {
        return;
    }

    if !block_info_has_face(block_info, dir) {
        mesh_output.primitives[dir*2u + 0u].cull = true;
        mesh_output.primitives[dir*2u + 1u].cull = true;
        return;
    }

    let block_posf = vec3f(f32(block_pos.x), f32(block_pos.y), f32(block_pos.z));
    for (var vi = 0u; vi < 4u; vi++) {
        let vertex = &mesh_output.vertices[dir*4 + vi];
        (*vertex).texcoord = get_cube_uv(vi);
        (*vertex).position = world.view_projection_matrix * vec4f(get_cube_vertex((*vertex).texcoord, dir) + block_posf + imm.chunk_offset, 1.);
    }

    mesh_output.primitives[dir*2u + 0u].indices = vec3u(dir*4) + vec3u(0,1,2);
    mesh_output.primitives[dir*2u + 1u].indices = vec3u(dir*4) + vec3u(2,1,3);

    let model = block_info_model(block_info);
    let face: BlockModelFaceData = block_model_get_face_data(model, dir);
    for (var pi = 0u; pi < 2u; pi++) {
        let prim = &mesh_output.primitives[dir*2u + pi];
        (*prim).cull = false;
        (*prim).tint_index = face.tint_index;
        (*prim).texture_index = face.texture_index;
        (*prim).face_tint = face.face_tint;
    }
}

@mesh(mesh_output)
@workgroup_size(6)
fn ms(
    @builtin(local_invocation_id) invocation_id: vec3u,
    @builtin(workgroup_id) workgroup_id: vec3u,
) {
    if invocation_id.x == 0 {
        mesh_output.vertex_count = 24;
        mesh_output.primitive_count = 12;
    }
    let block_pos = vec3i(i32(workgroup_id.x), i32(workgroup_id.y), i32(workgroup_id.z));
    run_mesh_shader(block_pos, invocation_id.x);
}

// Directional fake shading on blocks for each direction, same as minecraft
const lights: array<f32, 6> = array(0.6, 0.6, 1.0, 0.5, 0.8, 0.8);
const plains_grass_tint: vec4f = vec4f(0.5686274509803921, 0.7411764705882353, 0.34901960784313724, 1.);

const occlusion_levels: array<f32, 4> = array(1.0, 0.8, 0.6, 0.4);
@fragment fn fs(vertex: VertexOutput, primitive: PrimitiveInput) -> @location(0) vec4f {
    var tex = textureSample(texture, texture_sampler, vertex.texcoord, primitive.texture_index);

    var light = 1.;
    if primitive.face_tint < 6 {
        light = lights[primitive.face_tint];
    }
    tex = vec4f(tex.rgb * light, tex.a);

    if primitive.tint_index != 0 {
        tex *= plains_grass_tint;
    }
    if ENABLE_CUTOUT && tex.a < 0.5 {
        discard;
    }
    return tex;
}
