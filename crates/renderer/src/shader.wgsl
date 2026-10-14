struct Globals {
    // Canvas pixels -> clip space.
    canvas_to_clip: mat4x4<f32>,
    paper: vec4<f32>,
    // xy = document size in canvas pixels.
    doc_size: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var tile_tex: texture_2d<f32>;
@group(1) @binding(1) var tile_sampler: sampler;

const TILE_SIZE: f32 = 256.0;

// Two triangles covering the unit square.
fn corner(i: u32) -> vec2<f32> {
    var c = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
    );
    return c[i];
}

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_paper(@builtin(vertex_index) i: u32) -> VsOut {
    let p = corner(i) * globals.doc_size.xy;
    return VsOut(globals.canvas_to_clip * vec4(p, 0.0, 1.0), vec2(0.0));
}

@fragment
fn fs_paper(in: VsOut) -> @location(0) vec4<f32> {
    return globals.paper;
}

@vertex
fn vs_tile(@builtin(vertex_index) i: u32, @location(0) origin: vec2<f32>) -> VsOut {
    let uv = corner(i);
    let p = origin + uv * TILE_SIZE;
    return VsOut(globals.canvas_to_clip * vec4(p, 0.0, 1.0), uv);
}

@fragment
fn fs_tile(in: VsOut) -> @location(0) vec4<f32> {
    // Premultiplied alpha; blended with PREMULTIPLIED_ALPHA_BLENDING.
    return textureSample(tile_tex, tile_sampler, in.uv);
}
