enable wgpu_mesh_shader;
enable wgpu_binding_array;

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
    camera_position: vec3f,
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
}

struct ChunkData {
    offset: vec3f,
    padding: u32,
    blocks: array<u32, 4096>,
}

@group(0) @binding(0) var<uniform> world: WorldUniform;
@group(1) @binding(0) var texture_sampler: sampler;
@group(1) @binding(1) var texture: texture_2d_array<f32>;
@group(1) @binding(2) var<storage, read> models: array<BlockModel>;
@group(2) @binding(0) var<storage, read> all_chunks_data: binding_array<ChunkData>;

fn block_info_model(block_info: u32) -> u32 {
    return block_info & 0xFFFF;
}

fn block_info_group_faces(block_info: u32) -> u32 {
    return (block_info >> 22) & 0x3F;
}

fn block_info_has_any_group_face(block_info: u32) -> bool {
    return (block_info & 0xFC00000u) != 0;
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

struct DirectionTasks {
    task_count: u32,
    // Each task is 8 bits (from lsb):
    //   6 bits to store the work group block offset (2 bits per axis)
    //   2 bits of padding
    // Storing 4 task per u32, that's 16 u32s for 64 tasks
    tasks: array<u32, 16>,
}

struct TaskPayload {
    chunk_index: u32,
    // For each direction there is a list of tasks
    tasks: array<DirectionTasks, 6>,
}
var<task_payload> taskPayload: TaskPayload;
var<workgroup> required_faces_per_task: array<u32, 64>;

fn make_camera_culled_faces_mask(min_pos: vec3f, max_pos: vec3f) -> u32 {
    var mask: u32 = 0;
    if world.camera_position.x > min_pos.x {
        mask |= 1u << 0u;
    }
    if world.camera_position.x < max_pos.x {
        mask |= 1u << 1u;
    }
    if world.camera_position.y > min_pos.y {
        mask |= 1u << 2u;
    }
    if world.camera_position.y < max_pos.y {
        mask |= 1u << 3u;
    }
    if world.camera_position.z > min_pos.z {
        mask |= 1u << 4u;
    }
    if world.camera_position.z < max_pos.z {
        mask |= 1u << 5u;
    }
    return mask;
}

// Conservative frustum test of a 4x4x4 group: culled only if all 8 corners
// are outside the same clip plane
fn is_group_visible(min_pos: vec3f) -> bool {
    let base = world.view_projection_matrix * vec4f(min_pos, 1.);
    let dx = world.view_projection_matrix[0] * 4.;
    let dy = world.view_projection_matrix[1] * 4.;
    let dz = world.view_projection_matrix[2] * 4.;
    var outside = 0x3Fu;
    for (var i = 0u; i < 8u; i++) {
        let c = base + dx * f32(i & 1u) + dy * f32((i >> 1u) & 1u) + dz * f32(i >> 2u);
        var m = 0u;
        if c.x < -c.w { m |= 1u; }
        if c.x >  c.w { m |= 2u; }
        if c.y < -c.w { m |= 4u; }
        if c.y >  c.w { m |= 8u; }
        if c.z <  0.  { m |= 16u; }
        if c.z >  c.w { m |= 32u; }
        outside &= m;
    }
    return outside == 0u;
}

@task
@payload(taskPayload)
@workgroup_size(4,4,4)
fn ts_main(
    @builtin(local_invocation_id) invocation_id: vec3u,
    @builtin(local_invocation_index) invocation_idx: u32,
    @builtin(workgroup_id) workgroup_id: vec3u,
    @builtin(num_workgroups) num_workgroups: vec3u,
) -> @builtin(mesh_task_size) vec3u {
    let chunk_index = workgroup_id.x * num_workgroups.y * num_workgroups.z + workgroup_id.y * num_workgroups.z + workgroup_id.z;
    let chunk_offset = all_chunks_data[chunk_index].offset;
    let blocks = &all_chunks_data[chunk_index].blocks;

    let y_slice = invocation_id.y;
    let task_idx = invocation_id.z * 16 + invocation_id.y * 4 + invocation_id.x;
    let task_blocks_offset = chunk_offset + vec3f(invocation_id) * vec3f(4.);
    let face_camera_mask = make_camera_culled_faces_mask(task_blocks_offset, task_blocks_offset + vec3f(4.));
    required_faces_per_task[task_idx] = block_info_group_faces((*blocks)[task_idx * 64]) & face_camera_mask;
    if required_faces_per_task[task_idx] != 0 && !is_group_visible(task_blocks_offset) {
        required_faces_per_task[task_idx] = 0;
    }

    workgroupBarrier();
    if invocation_idx < 6 {
        let face_dir = invocation_idx;
        let face_mask = 1u << face_dir;

        let face_task = &taskPayload.tasks[face_dir];
        var task_counter = 0u;
        for (var task2 = 0u; task2 < 64; task2++) {
            if (required_faces_per_task[task2] & face_mask) == 0 { continue; }

            let word = task_counter >> 2;
            let bit_offset = (task_counter & 0x3) * 8;
            if bit_offset == 0 {
                // So we don't have to zero out all value before hand we do
                // each time we reach a new word
                (*face_task).tasks[word] = task2;
            }
            else {
                (*face_task).tasks[word] |= task2 << bit_offset;
            }
            
            task_counter += 1u;
        }

        (*face_task).task_count = task_counter;
    }
    workgroupBarrier();
    if invocation_idx == 0 {
        taskPayload.chunk_index = chunk_index;
        var total_tasks = 0u;
        for (var dir = 0u; dir < 6; dir++) {
            total_tasks += taskPayload.tasks[dir].task_count;
        }
        return vec3u(total_tasks,1,1);
    } 
    return vec3u(0);
}

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) world_pos: vec3f,
}

struct PrimitiveOutput {
    @builtin(triangle_indices) indices: vec3<u32>,
    @builtin(cull_primitive) cull: bool,
    @interpolate(flat, either) @per_primitive @location(1) face_direction: u32,
    @interpolate(flat, either) @per_primitive @location(2) tint_index: u32,
    @interpolate(flat, either) @per_primitive @location(3) texture_index: u32,
    @interpolate(flat, either) @per_primitive @location(4) face_tint: u32,
}
struct PrimitiveInput {
    @interpolate(flat, either) @per_primitive @location(1) face_direction: u32,
    @interpolate(flat, either) @per_primitive @location(2) tint_index: u32,
    @interpolate(flat, either) @per_primitive @location(3) texture_index: u32,
    @interpolate(flat, either) @per_primitive @location(4) face_tint: u32,
}

struct MeshOutput {
    @builtin(vertices) vertices: array<VertexOutput, 125>,
    @builtin(primitives) primitives: array<PrimitiveOutput, 128>,
    @builtin(vertex_count) vertex_count: u32,
    @builtin(primitive_count) primitive_count: u32,
}
var<workgroup> mesh_output: MeshOutput;

struct ExtractedGroupTaskData {
    dir: u32,
    pos: vec3u,
}

fn extract_group_task_data(idx: u32) -> ExtractedGroupTaskData {
    var moving_idx = idx;
    var dir: u32 = 0;
    // If dir goes beyond 6 there is an error somewhere, but better no hang forever
    while dir < 6 && taskPayload.tasks[dir].task_count <= moving_idx {
        moving_idx -= taskPayload.tasks[dir].task_count;
        dir += 1;
    }
    let payload = taskPayload.tasks[dir].tasks[moving_idx>>2] >> ((moving_idx&0x3) * 8);

    var out: ExtractedGroupTaskData;
    out.dir = dir;
    out.pos = vec3u(payload & 0x3, (payload>>2) & 0x3, (payload>>4) & 0x3);
    return out;
}

const DIR_FACE_VERTICES = array<array<vec3u,2>,6>(
    array(vec3u(26,1,31),vec3u(31,1,6)),
    array(vec3u(0,25,5),vec3u(5,25,30)),
    array(vec3u(30,31,5),vec3u(5,31,6)),
    array(vec3u(0,1,25),vec3u(25,1,26)),
    array(vec3u(25,26,30),vec3u(30,26,31)),
    array(vec3u(1,0,6),vec3u(6,0,5)),
);

// Each mesh work group handles a 4x4x4 slice of the chunk
// This means (4+1)^3=125 vertices and 4^3*2=128 primitives
@mesh(mesh_output)
@payload(taskPayload)
@workgroup_size(5,5,5)
fn ms(
    @builtin(local_invocation_id) invocation_id: vec3u,
    @builtin(local_invocation_index) invocation_idx: u32,
    @builtin(workgroup_id) workgroup_id: vec3u,
) {
    let group_task_data = extract_group_task_data(workgroup_id.x);
    let dir = group_task_data.dir;

    let chunk_index = taskPayload.chunk_index;
    let chunk_data = &all_chunks_data[chunk_index];

    let group_pos = group_task_data.pos;
    let group_base_block_pos = group_pos * vec3u(4);
    let group_base_block_idx = (group_pos.x + group_pos.y*4 + group_pos.z*16)*64;

    let global_base_group_pos = vec3f(group_base_block_pos) + (*chunk_data).offset;
    let global_base_group_pos_clip_space = world.view_projection_matrix * vec4f(global_base_group_pos, 1.);

    if invocation_idx == 0 {
        mesh_output.vertex_count = 125;
        mesh_output.primitive_count = 128;
    }

    let vertex_pos = group_base_block_pos + invocation_id;
    let global_vertex_pos = vec3f(vertex_pos) + (*chunk_data).offset;
    let vertex_idx = invocation_idx;

    {
        let vertex = &mesh_output.vertices[vertex_idx];
        (*vertex).world_pos = global_vertex_pos;
        (*vertex).position = global_base_group_pos_clip_space + world.view_projection_matrix[0] * f32(invocation_id.x) + world.view_projection_matrix[1] * f32(invocation_id.y) + world.view_projection_matrix[2] * f32(invocation_id.z);
    }

    let block_pos = vertex_pos;
    let block_pos_in_group = invocation_id;
    if !all(block_pos_in_group < vec3u(4)) { return; }
    let block_offset_idx = invocation_id.x + invocation_id.y*4 + invocation_id.z*16;
    let block_idx = group_base_block_idx + block_offset_idx;
    let block_info = (*chunk_data).blocks[block_idx];

    let prim_offset = block_offset_idx * 2;

    if !block_info_has_face(block_info, dir) {
        mesh_output.primitives[prim_offset+0u].cull = true;
        mesh_output.primitives[prim_offset+1u].cull = true;
        return;
    }

    mesh_output.primitives[prim_offset+0u].indices = vec3u(vertex_idx) + DIR_FACE_VERTICES[dir][0];
    mesh_output.primitives[prim_offset+1u].indices = vec3u(vertex_idx) + DIR_FACE_VERTICES[dir][1];

    let model = block_info_model(block_info);
    let face: BlockModelFaceData = block_model_get_face_data(model, dir);
    for (var pi = 0u; pi < 2u; pi++) {
        let prim = &mesh_output.primitives[prim_offset+pi];
        (*prim).cull = false;
        (*prim).face_direction = dir;
        (*prim).tint_index = face.tint_index;
        (*prim).texture_index = face.texture_index;
        (*prim).face_tint = face.face_tint;
    }
}

// Directional fake shading on blocks for each direction, same as minecraft
const lights: array<f32, 6> = array(0.6, 0.6, 1.0, 0.5, 0.8, 0.8);
const plains_grass_tint: vec4f = vec4f(0.5686274509803921, 0.7411764705882353, 0.34901960784313724, 1.);

const occlusion_levels: array<f32, 4> = array(1.0, 0.8, 0.6, 0.4);

fn get_uv(world_pos: vec3f, dir: u32) -> vec2f {
    let uv = ((world_pos % vec3f(1.)) + vec3f(1.)) % vec3f(1.);
    switch (dir) {
    case 0u: { return uv.zy * vec2(-1.,-1.) + vec2(1.,1.); }
    case 1u: { return uv.zy * vec2(1.,-1.) + vec2(0.,1.); }
    case 2u: { return uv.xz * vec2(1.,1.) + vec2(0.,0.); }
    case 3u: { return uv.xz * vec2(-1.,1.) + vec2(1.,0.); }
    case 4u: { return uv.xy * vec2(1.,-1.) + vec2(0.,1.); }
    default: { return uv.xy * vec2(-1.,-1.) + vec2(1.,1.); }
    }
}

@fragment fn fs(vertex: VertexOutput, primitive: PrimitiveInput) -> @location(0) vec4f {
    var light = 1.;
    if primitive.face_tint < 6 {
        light = lights[primitive.face_tint];
    }

    let texcoord = get_uv(vertex.world_pos, primitive.face_direction);

    var tex = textureSample(texture, texture_sampler, texcoord, primitive.texture_index);
    tex = vec4f(tex.rgb * light, tex.a);

    if primitive.tint_index != 0 {
        tex *= plains_grass_tint;
    }
    if ENABLE_CUTOUT && tex.a < 0.5 {
        discard;
    }
    return tex;
}
