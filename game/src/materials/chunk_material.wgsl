const pi = radians(180.0);
const tau = radians(360.0);

override ENABLE_CUTOUT: bool;
override AMBIENT_OCCLUSION_DEBUG: bool = false;

struct WorldUniform {
    view_projection_matrix: mat4x4f,
    time: f32,
};

@group(0) @binding(0) var<uniform> world: WorldUniform;
@group(1) @binding(0) var texture_sampler: sampler;
@group(1) @binding(1) var texture: texture_2d_array<f32>;

struct Immediates {
    chunk_offset: vec3f,
};

var<immediate> imm: Immediates;

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) texcoord: vec2f,
    @interpolate(flat) @location(1) tint_index: u32,
    @interpolate(flat) @location(2) texture_index: u32,
    @interpolate(flat) @location(3) face_tint: u32,
};

@vertex fn vs(
    @location(0) vertex_pos: vec3f,
    @location(1) vertex_uv: vec2f,
    @location(2) bits_data: u32,
) -> VertexOutput {
    var output: VertexOutput;
    output.position = world.view_projection_matrix * vec4f(vertex_pos + imm.chunk_offset, 1.0);
    output.texcoord = vertex_uv;
    output.tint_index = (bits_data >> 0) & 0x3;
    output.texture_index = (bits_data >> 2) & 0x7f;
    output.face_tint = (bits_data >> 9) & 0x7;
    return output;
}

// Directional fake shading on blocks for each direction, same as minecraft
const lights: array<f32, 6> = array(0.6, 0.6, 1.0, 0.5, 0.8, 0.8);
const plains_grass_tint: vec4f = vec4f(0.5686274509803921, 0.7411764705882353, 0.34901960784313724, 1.);

const occlusion_levels: array<f32, 4> = array(1.0, 0.8, 0.6, 0.4);
@fragment fn fs(input: VertexOutput) -> @location(0) vec4f {
    var tex = textureSample(texture, texture_sampler, input.texcoord, input.texture_index);

    var light = 1.;
    if input.face_tint < 7 {
        light = lights[input.face_tint];
    }
    tex = vec4f(tex.rgb * light, tex.a);

    if input.tint_index != 0 {
        tex *= plains_grass_tint;
    }
    if ENABLE_CUTOUT && tex.a < 10e-5 {
        discard;
    }
    return tex;
}
