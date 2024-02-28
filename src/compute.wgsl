const max: u32 = 1024;

@group(0)
@binding(0)
var<storage, read> points: array<vec2<f32>>;

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
    let index = (global_id.y * 1600) + global_id.x;
    let iter_count = mandelbrot_iterations(points[index]);
    iterations[index] = iter_count;
}
