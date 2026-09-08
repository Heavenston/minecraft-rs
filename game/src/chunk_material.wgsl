const pi = radians(180.0);
const tau = radians(360.0);

fn get_cube_uv(vertex_index: u32) -> vec2f {
    return vec2f(f32(vertex_index & 1u), f32((vertex_index >> 1u) & 1u));
}
fn get_cube_vertex(uv: vec2f, dir: u32) -> vec3f {
    let u = uv.x;
    let v = uv.y;
    switch (dir) {
        case 0u: { return vec3f(1.0, v, 1.0 - u); } // +X
        case 1u: { return vec3f(0.0, v, u); }       // -X
        case 2u: { return vec3f(u, 1.0, 1.0 - v); } // +Y
        case 3u: { return vec3f(u, 0.0, v); }       // -Y
        case 4u: { return vec3f(u, v, 1.0); }       // +Z
        default: { return vec3f(1.0 - u, v, 0.0); } // -Z
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
@group(1) @binding(1) var texture: texture_2d<f32>;

struct Immediates {
    position: vec3f,
    direction: u32,
};

var<immediate> imm: Immediates;

fn extract_offset(data: u32) -> vec3f {
    var output: vec3f;
    output.x = f32((data >> 8) & 0x0F);
    output.y = f32((data >> 4) & 0x0F);
    output.z = f32((data >> 0) & 0x0F);
    return output;
}

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) texcoord: vec2f,
};

@vertex fn vs(
    @location(0) face_data: u32,
    @builtin(vertex_index) vertex_index: u32,
) -> VertexOutput {
    let uv = get_cube_uv(vertex_index);
    let pos = get_cube_vertex(uv, imm.direction) + extract_offset(face_data) + imm.position;

    var output: VertexOutput;
    output.position = world.view_projection_matrix * vec4f(pos, 1.0);
    output.texcoord = uv;
    return output;
}

const lights: array<f32, 6> = array(0.77694196, 0.9730581, 0.8504855, 0.8995145, 0.94854355, 0.875);

@fragment fn fs(input: VertexOutput) -> @location(0) vec4f {
    var tex = textureSample(texture, texture_sampler, input.texcoord);
    tex = vec4f(tex.rgb * lights[imm.direction], tex.a);
    return tex;
}
