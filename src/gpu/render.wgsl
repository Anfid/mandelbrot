struct VertexOutput {
    @location(0) coordinates: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(in_vertex_index) / 2 * 2 - 1);
    let y = f32(i32(in_vertex_index) % 2 * 2 - 1);
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.coordinates = out.position.xy;
    return out;
}

struct Parameters {
    dimensions: vec2<u32>,
    max: u32,
}

@group(0)
@binding(0)
var<storage, read> params: Parameters;

@group(0)
@binding(1)
var r_color: texture_2d<u32>;

fn colors(i: u32) -> vec3<f32> {
    let p = 2.0 * radians(180.0) / 3.0;
    var buffer: u32 = 20;
    let cutoff = 0.2;

    if i == params.max {
        return vec3<f32>(0.0, 0.0, 0.0);
    } else if i < buffer {
        let n = f32(i) / f32(buffer - 1);
        let r = 1 - (1 + cutoff) / (2 - cutoff);
        let g = 1 - (cos(p) + cutoff) / (2 - cutoff);
        let b = 1 - (cos(2.0 * p) + cutoff) / (2 - cutoff);
        return vec3<f32>(1.0 - n * r, 1.0 - n * g, 1.0 - n * b);
    } else {
        let n = sqrt(f32(i - buffer) / 5.0);
        let r = (cos(n) + cutoff) / (2 - cutoff);
        let g = (cos(n + p) + cutoff) / (2 - cutoff);
        let b = (cos(n + 2.0 * p) + cutoff) / (2 - cutoff);
        return vec3<f32>(r, g, b);
    }
}

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    let coords = vec2<f32>(vertex.coordinates.x, -vertex.coordinates.y);
    let point = vec2<u32>((coords + 1.0) / 2.0 * vec2<f32>(params.dimensions));
    let tex = textureLoad(r_color, point, 0);

    return vec4<f32>(colors(tex.x), 1.0);
}
