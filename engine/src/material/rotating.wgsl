const pi = radians(180.0);
const tau = radians(360.0);

struct WorldUniform {
    view_projection_matrix: mat4x4f,
    time: f32,
};

@group(0) @binding(0) var<uniform> world: WorldUniform;

struct Uniform {
    color: vec4f,
    speed: f32,
};

@group(1) @binding(0) var<uniform> uni: Uniform;

@vertex fn vs(@builtin(vertex_index) vertexIndex : u32) -> @builtin(position) vec4f {
    let pos = array(
        vec2f( 0.0,  0.5),  // top center
        vec2f(-0.5, -0.5),  // bottom left
        vec2f( 0.5, -0.5)   // bottom right
    );

    let speed = (uni.speed / 60f) * tau;
    let angle = world.time * speed;
    let c = cos(angle);
    let s = sin(angle);
    let matrix: mat2x2f = mat2x2f(c, -s, s, c);

    return vec4f(pos[vertexIndex] * matrix, 0.0, 1.0);
}

@fragment fn fs() -> @location(0) vec4f {
    return uni.color;
}
