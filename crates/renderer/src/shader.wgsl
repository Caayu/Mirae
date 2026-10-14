struct Globals {
    // Canvas pixels -> clip space.
    canvas_to_clip: mat4x4<f32>,
    // Straight-alpha paper color.
    paper: vec4<f32>,
    // xy = document size in canvas pixels.
    doc_size: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var canvas_tex: texture_2d<f32>;
@group(0) @binding(2) var canvas_sampler: sampler;

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
fn vs_canvas(@builtin(vertex_index) i: u32) -> VsOut {
    let uv = corner(i);
    let p = uv * globals.doc_size.xy;
    return VsOut(globals.canvas_to_clip * vec4(p, 0.0, 1.0), uv);
}

@fragment
fn fs_canvas(in: VsOut) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(canvas_tex, 0));
    let t = in.uv * size;
    // Derivatives in uniform control flow, before any branch.
    let duv_dx = dpdx(in.uv);
    let duv_dy = dpdy(in.uv);
    let w = max(fwidth(t), vec2(1e-5));
    var uv = in.uv;
    if (max(w.x, w.y) < 1.0) {
        // Magnifying: anti-aliased nearest. Each texel stays a flat square and
        // only the band within one screen pixel of a texel border is blended,
        // so pixels are crisp at any zoom, uniform in size, and do not alias
        // when the view is rotated.
        let base = floor(t - 0.5);
        let f = clamp((fract(t - 0.5) - 0.5) / w + 0.5, vec2(0.0), vec2(1.0));
        uv = (base + 0.5 + f) / size;
    }
    // Minifying: hardware trilinear over the box-filtered mip chain.
    let c = textureSampleGrad(canvas_tex, canvas_sampler, uv, duv_dx, duv_dy);
    let paper = vec4(globals.paper.rgb * globals.paper.a, globals.paper.a);
    // Premultiplied "over".
    return c + paper * (1.0 - c.a);
}
