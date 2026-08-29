var<immediate> color: vec4f;

@vertex fn vs(
    @builtin(vertex_index) vertexIndex : u32
) -> @builtin(position) vec4f {
    let pos = array(
        vec2f(-1.0, -1.0),
        vec2f( 1.0, -1.0),
        vec2f(-1.0,  1.0),
        vec2f( 1.0,  1.0),
    );

    return vec4f(pos[vertexIndex], 0.0, 1.0);
}

@fragment fn fs() -> @location(0) vec4f {
    return color;
}
