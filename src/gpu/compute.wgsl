const max: u32 = 4294967295u;

struct Parameters {
    iteration_limit: u32,
    reset: u32,
    size: vec2<u32>,
    words: array<u32>,
}

@group(0)
@binding(0)
var<storage, read> params: Parameters;

@group(0)
@binding(1)
var<storage, read_write> iterations: array<u32>;

@group(0)
@binding(2)
var<storage, read_write> intermediate: array<u32>;

// Calculate mandelbrot iterations
//
// Requires arena to have enough space for 8 wide numbers.
// Requires first 4 numbers in the arena to be pre-initialized the following params before the call:
// 1: origin X
// 2: origin Y
// 3: iteration X
// 4: iteration Y
fn wide_mandelbrot(start_iter: u32, iteration_limit: u32) -> u32 {
    let origin_x = NumView(0u * word_count);
    let origin_y = NumView(1u * word_count);

    let x = NumView(2u * word_count);
    let y = NumView(3u * word_count);

    let x2 = NumView(4u * word_count);
    let y2 = NumView(5u * word_count);

    let tmpx = NumView(6u * word_count);
    let tmpy = NumView(7u * word_count);

    // x2 = x * x
    wide_mul(x, x, x2, tmpx);
    // y2 = y * y
    wide_mul(y, y, y2, tmpy);

    var i: u32 = start_iter;
    wide_clone(x2, tmpx);
    while i < max && i < start_iter + iteration_limit && wide_cmp(wide_add(tmpx, y2), 4) == -1 {
        // y *= 2
        wide_double(y);

        // tmpy = y
        wide_clone(y, tmpy);

        // y = x * tmpy
        wide_mul(x, tmpy, y, tmpx);

        // y += origin_y
        wide_add(y, origin_y);

        // x = x2
        wide_clone(x2, x);

        // x -= y2
        wide_sub(x, y2);

        // x += origin_x
        wide_add(x, origin_x);

        // x2 = x * x
        wide_mul(x, x, x2, tmpx);

        // y2 = y * y
        wide_mul(y, y, y2, tmpy);

        i++;
        wide_clone(x2, tmpx);
    }

    return i;
}

@compute
@workgroup_size(64)
fn main(
    @builtin(global_invocation_id) global_id: vec3<u32>,
    @builtin(local_invocation_id) local_id: vec3<u32>,
) {
    let pixel_x = global_id.x;
    let pixel_y = global_id.y;
    let index = (pixel_y * params.size.x) + pixel_x;

    // Declare origin_x, origin_y and step
    let origin_x = NumView(0u * word_count);
    let origin_y = NumView(1u * word_count);
    let step = NumView(2u * word_count);

    // Initialize origin_x, origin_y and step from parameter buffer
    for (var i = 0u; i < 3 * word_count; i++) {
        arena[i] = params.words[i];
    }

    let offset_x = NumView(3u * word_count);
    let offset_y = NumView(4u * word_count);

    let wide_pixel_x = NumView(5u * word_count);
    let wide_pixel_y = NumView(6u * word_count);
    let tmp = NumView(7u * word_count);

    // wide_pixel_x = pixel_x
    wide_from_u32(pixel_x, wide_pixel_x);
    // wide_pixel_y = pixel_y
    wide_from_u32(pixel_y, wide_pixel_y);

    // offset_x = step * pixel_x
    wide_mul(wide_pixel_x, step, offset_x, tmp);
    // offset_y = step * pixel_y
    wide_mul(wide_pixel_y, step, offset_y, tmp);

    // origin_x += offset_x
    wide_add(origin_x, offset_x);
    // origin_y += offset_y
    wide_add(origin_y, offset_y);

    var iterstart: u32;
    if params.reset != 0 {
        iterstart = 0u;
        // Set intermediate X and Y results to origin
        let x = NumView(2u * word_count);
        let y = NumView(3u * word_count);
        wide_clone(origin_x, x);
        wide_clone(origin_y, y);
    } else {
        iterstart = iterations[index];
        // Read intermediate X and Y results
        for (var i = 0u; i < 2 * word_count; i++) {
            arena[2 * word_count + i] = intermediate[2 * index * word_count + i];
        }
    }

    let iteration_limit = params.iteration_limit;
    let iter_count = wide_mandelbrot(iterstart, iteration_limit);

    // Write intermediate X and Y results to continue on the next iteration
    for (var i = 0u; i < 2 * word_count; i++) {
        intermediate[2 * index * word_count + i] = arena[2 * word_count + i];
    }

    iterations[index] = iter_count;
}

// ===== Bignum =====

// Override variables aren't available yet, temporary bandaid
// Tracking issue: https://github.com/gfx-rs/wgpu/issues/4484
const word_count: u32 = 5;

const arena_size: u32 = word_count * 8;
var<private> arena: array<u32, arena_size>;

struct NumView {
    idx: u32,
}

// Mutates `left`, incrementing it by `right`. Returns the handle to the left number
fn wide_add(left: NumView, right: NumView) -> NumView {
    var carry = 0u;
    for (var i = 0u; i < word_count; i++) {
        let res = carrying_add(arena[left.idx + i], arena[right.idx + i], carry);
        arena[left.idx + i] = res.x;
        carry = res.y;
    }
    return left;
}

// Mutates `left`, decrementing it by `right`. Returns the handle to the left number
fn wide_sub(left: NumView, right: NumView) -> NumView {
    var borrow = 0u;
    for (var i = 0u; i < word_count; i++) {
        let res = borrowing_sub(arena[left.idx + i], arena[right.idx + i], borrow);
        arena[left.idx + i] = res.x;
        borrow = res.y;
    }
    return left;
}

// Mutates `num` by flipping its sign. Returns the handle to the mutated number
fn wide_neg(num: NumView) -> NumView {
    var carry = 1u;
    for (var idx = num.idx; idx < num.idx + word_count; idx++) {
        let res = carrying_add(~arena[idx], 0u, carry);
        arena[idx] = res.x;
        carry = res.y;
    }
    return num;
}

// Mutates `out` by writing the result of multiplication of `left` and `right` to it. Operation
// requires extra space to store intermediate results, which must be provided via `tmp`.
// Returns the handle to `out`
fn wide_mul(left: NumView, right: NumView, out: NumView, tmp: NumView) -> NumView {
    for (var idx = out.idx; idx < out.idx + word_count; idx++) {
        arena[idx] = 0u;
    }

    let lneg = sign(wide_floor(left)) == -1;
    let rneg = sign(wide_floor(right)) == -1;

    var neg_carry = 1u;

    for (var i = 0u; i < word_count; i++) {
        var lword = arena[left.idx + i];
        if lneg {
            let res = carrying_add(~lword, 0u, neg_carry);
            lword = res.x;
            neg_carry = res.y;
        }
        wide_clone(right, tmp);
        if rneg {
            wide_neg(tmp);
        }

        var mul_carry = 0u;
        for (var j = 0u; j < word_count; j++) {
            let res = carrying_mul(lword, arena[tmp.idx + j], mul_carry);
            arena[tmp.idx + j] = res.x;
            mul_carry = res.y;
        }
        let shift = word_count - i - 1;
        wide_shr_words(tmp, shift);
        if mul_carry != 0u {
            arena[tmp.idx + i + 1] = mul_carry;
        }
        wide_add(out, tmp);
    }

    if u32(rneg) + u32(lneg) == 1u {
        wide_neg(out);
    }

    return out;
}

// Mutates `num` by doubling it. Returns the handle to the mutated number
fn wide_double(num: NumView) -> NumView {
    var shift_carry = 0u;
    for (var idx = num.idx; idx < num.idx + word_count; idx++) {
        let tmp = (arena[idx] << 1u) + shift_carry;
        shift_carry = arena[idx] >> 31u;
        arena[idx] = tmp;
    }
    return num;
}

// Mutates `num` by right shifting all its words by `shift`. Returns the handle to the mutated number
fn wide_shr_words(num: NumView, shift: u32) -> NumView {
    // Since numbers are stored in LE format, bit rotate has to shift words to the left instead
    for (var idx = num.idx; idx < num.idx + word_count; idx++) {
        let src = idx + shift;
        // Write zero in case of overflow or if `src` is out of bounds of `num`
        arena[idx] = select(arena[src], 0u, src < idx || src >= num.idx + word_count);
    }
    return num;
}

// Compares wide fraction `left` to i32 `right`. Returns -1 if `left` is less than `right`, 0 if
// numbers are equal and 1 if `left` is greater than `right`
fn wide_cmp(left: NumView, right: i32) -> i32 {
    let whole = wide_floor(left);
    if whole < right {
        return -1;
    } else if whole > right {
        return 1;
    } else {
        // If whole numbers are equal, check if fraction part is non-zero
        for (var idx = left.idx + 1; idx < left.idx + word_count; idx++) {
            if arena[idx] != 0 {
                return 1;
            }
        }
        return 0;
    }
}

// Returns the whole part of the wide number, dropping the fraction
fn wide_floor(num: NumView) -> i32 {
    return bitcast<i32>(arena[num.idx + word_count - 1]);
}

// Mutates `dst` by writing the contents of `src` to it
fn wide_clone(src: NumView, dst: NumView) {
    for (var i = 0u; i < word_count; i++) {
        arena[dst.idx + i] = arena[src.idx + i];
    }
}

// Initializes `dst` with the value of i32 `src`
fn wide_from_u32(src: u32, dst: NumView) {
    arena[dst.idx + word_count - 1] = src;
    for (var idx = dst.idx; idx < dst.idx + word_count - 1; idx++) {
        arena[idx] = 0u;
    }
}

// ===== Bignum helper functions =====

// Add with carry on overflow. `carry` MUST be 0 or 1
fn carrying_add(left: u32, right: u32, carry: u32) -> vec2<u32> {
    let sum = left + right + carry;
    return vec2<u32>(sum, u32(sum < left || (sum == left && carry != 0)));
}

// Add with borrow on underflow. `borrow` MUST be 0 or 1
fn borrowing_sub(left: u32, right: u32, borrow: u32) -> vec2<u32> {
    let diff = left - right - borrow;
    return vec2<u32>(diff, u32(diff > left || (diff == left && borrow != 0)));
}

// Multiply with carry on overflow
fn carrying_mul(left: u32, right: u32, carry: u32) -> vec2<u32> {
    let x0 = 0xffff & left;
    let x1 = left >> 16;
    let y0 = 0xffff & right;
    let y1 = right >> 16;

    let p00 = x0 * y0;
    let p01 = x0 * y1;
    let p10 = x1 * y0;
    let p11 = x1 * y1;

    let middle = p10 + (p00 >> 16) + (0xffff & p01);

    let value = (middle << 16) | (0xffff & p00);
    let carried_res = carrying_add(value, carry, 0u);

    let mul_carry = p11 + (middle >> 16) + (p01 >> 16) + carried_res.y;

    return vec2<u32>(carried_res.x, mul_carry);
}
