const pi = radians(180.0);
const tau = radians(360.0);

override ENABLE_CUTOUT: bool;

//  /-----+
//  | 2   3
//  | 
//  + 0   1
//  /-----+
//  | 0   2
//  | 
//  + 1   3
// 2 -> 0
// 3 -> 2
// 0 -> 1
// 1 -> 3
const vertex_inv_map: array<u32, 4> = array(1,3,0,2);

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

@group(0) @binding(0) var<uniform> world: WorldUniform;
@group(1) @binding(0) var texture_sampler: sampler;
@group(1) @binding(1) var texture: texture_2d_array<f32>;

struct Immediates {
    position: vec3f,
    direction: u32,
};

var<immediate> imm: Immediates;

fn extract_offset(data: u32) -> vec3f {
    var output: vec3f;
    output.x = f32((data >> 0) & 0x0F);
    output.y = f32((data >> 4) & 0x0F);
    output.z = f32((data >> 8) & 0x0F);
    return output;
}

// const occlusion_levels: array<f32, 4> = array(1.0, 0.8, 0.6, 0.4);
const occlusion_levels: array<f32, 4> = array(1.0, 0.666, 0.333, 0.);

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) texcoord: vec2f,
    @interpolate(flat) @location(1) tint_index: u32,
    @interpolate(flat) @location(2) texture_index: u32,
    @location(3) occlusion: f32,
};

@vertex fn vs(
    @location(0) face_data: u32,
    @builtin(vertex_index) vertex_index_: u32,
) -> VertexOutput {
    var vertex_index = vertex_index_;
    if (((face_data >> 24) & 0x3) + ((face_data >> 28) & 0x3)) > (((face_data >> 26) & 0x3) + ((face_data >> 30) & 0x3)) {
        vertex_index = vertex_inv_map[vertex_index];
    }

    var uv = get_cube_uv(vertex_index);
    let pos = get_cube_vertex(uv, imm.direction) + extract_offset(face_data) + imm.position;

    let uv_rotation = (face_data >> 21) & 0x3;
    let uv_flipped = (face_data >> 23) & 0x1;
    switch uv_rotation {
    case 1u: { uv = vec2f(1. - uv.y, uv.x); }
    case 2u: { uv = vec2f(1. - uv.x, 1. - uv.y); }
    case 3u: { uv = vec2f(uv.y, 1. - uv.x); }
    default: { }
    }

    if uv_flipped != 0u {
        uv.x = 1. - uv.x;
    }

    let ambient_occlusion = (face_data >> (24 + vertex_index * 2)) & 0x3;
    var output: VertexOutput;
    output.position = world.view_projection_matrix * vec4f(pos, 1.0);
    output.texcoord = uv;
    output.tint_index = (face_data >> 12) & 0x3;
    output.texture_index = (face_data >> 14) & 0x7f;
    output.occlusion = occlusion_levels[ambient_occlusion];
    return output;
}

// Directional fake shading on blocks for each direction, same as minecraft
const lights: array<f32, 6> = array(0.6, 0.6, 1.0, 0.5, 0.8, 0.8);

const plains_grass_tint: vec4f = vec4f(0.5686274509803921, 0.7411764705882353, 0.34901960784313724, 1.);

@fragment fn fs(input: VertexOutput) -> @location(0) vec4f {
    var tex = textureSample(texture, texture_sampler, input.texcoord, input.texture_index);
    let light = lights[imm.direction];
    tex = vec4f(tex.rgb * light * input.occlusion, tex.a);
    if input.tint_index != 0 {
        tex *= plains_grass_tint;
    }
    if ENABLE_CUTOUT && tex.a < 10e-5 {
        discard;
    }
    return tex;
}
