const max: u32 = 1024;

struct Parameters {
    coords: vec2<f32>,
    width: u32,
    height: u32,
    point_size: f32,
}

@group(0)
@binding(0)
var<storage, read> params: Parameters;

@group(0)
@binding(1)
var<storage, read_write> iterations: array<u32>;

fn mandelbrot_iterations(origin: vec2<f32>) -> u32 {
    var p = origin;
    var p2 = p * p;

    var i: u32 = 0u;
    while i < max && p2.x + p2.y < 4 {
        p.y = 2 * p.x * p.y + origin.y;
        p.x = p2.x - p2.y + origin.x;

        p2 = p * p;
        i++;
    }
    return i;
}

@compute
@workgroup_size(1)
fn main(
    @builtin(global_invocation_id) global_id: vec3<u32>,
) {
    let index = (global_id.y * params.width) + global_id.x;
    let iter_count = mandelbrot_iterations(params.coords + vec2<f32>(global_id.xy) * params.point_size);
    iterations[index] = iter_count;
}
