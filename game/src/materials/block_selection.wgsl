
fn get_cube_uv(vertex_index: u32) -> vec2f {
    return vec2f(
        f32(vertex_index & 1u),
        f32(1u - (vertex_index >> 1u)),
    );
}

fn get_cube_vertex(uv: vec2f, dir: u32) -> vec3f {
    let u = uv.x;
    let v = 1.0 - uv.y;
    switch (dir) {
        case 0u: { return vec3f(1.0    , v  , 1.0 - u); } // +X
        case 1u: { return vec3f(0.0    , v  ,       u); } // -X
        case 2u: { return vec3f(u      , 1.0, 1.0 - v); } // +Y
        case 3u: { return vec3f(u      , 0.0,       v); } // -Y
        case 4u: { return vec3f(u      , v  , 1.0    ); } // +Z
        default: { return vec3f(1.0 - u, v  , 0.0    ); } // -Z
    }
}


struct WorldUniform {
    view_projection_matrix: mat4x4f,
    time: f32,
};

@group(0) @binding(0) var<uniform> world: WorldUniform;

struct Immediates {
    position: vec3f,
};

var<immediate> imm: Immediates;

struct VertexOutput {
    @builtin(position) position: vec4f,
};

@vertex fn vs(
    @builtin(vertex_index) vertex_index: u32,
) -> VertexOutput {
    let dir = vertex_index / 6;
    var idx = vertex_index % 6;
    switch (idx) {
    case 3: { idx = 2; }
    case 4: { idx = 1; }
    case 5: { idx = 3; }
    default: { }
    }

    var output: VertexOutput;
    output.position = world.view_projection_matrix * vec4f(get_cube_vertex(get_cube_uv(idx), dir) + imm.position, 1);
    return output;
}

@fragment fn fs(input: VertexOutput) -> @location(0) vec4f {
    let s = (sin(world.time * 3.14) + 1.) / 16. + 0.0125;
    return vec4f(1,1,1,s);
}
