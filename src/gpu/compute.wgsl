struct Parameters {
    depth_limit: u32,
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
// Requires arena to have enough space for 7 wide numbers.
// Requires first 4 numbers in the arena to be pre-initialized the following params before the call:
// 1: origin X
// 2: origin Y
// 3: iteration X
// 4: iteration Y
fn wide_mandelbrot(start_iter: u32, depth_limit: u32) -> u32 {
    let origin_x = NumView(0u * word_count);
    let origin_y = NumView(1u * word_count);

    let x = NumView(2u * word_count);
    let y = NumView(3u * word_count);

    let x2 = NumView(4u * word_count);
    let y2 = NumView(5u * word_count);

    let tmp = NumView(6u * word_count);

    // x2 = x * x
    wide_square(x, x2);
    // y2 = y * y
    wide_square(y, y2);

    var i: u32 = start_iter;
    wide_clone(x2, tmp);
    while i < depth_limit && wide_cmp(wide_add(tmp, y2), 4) == -1 {
        // y = square(x2 + y2) - x2 - y2 + origin_y
        // x = x2 - y2 + origin_x

        // tmpy = y
        wide_clone(y, tmp);

        // tmpy += x
        wide_add(tmp, x);

        // y = tmpy * tmpy
        wide_square(tmp, y);

        // y -= y2
        wide_sub(y, y2);

        // y -= x2
        wide_sub(y, x2);

        // y += origin_y
        wide_add(y, origin_y);

        // x = x2
        wide_clone(x2, x);

        // x -= y2
        wide_sub(x, y2);

        // x += origin_x
        wide_add(x, origin_x);

        // x2 = x * x
        wide_square(x, x2);

        // y2 = y * y
        wide_square(y, y2);

        i++;
        wide_clone(x2, tmp);
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

    // offset_x = step * pixel_x
    wide_clone(step, offset_x);
    wide_mul_u32(offset_x, pixel_x);

    // offset_y = step * pixel_y
    wide_clone(step, offset_y);
    wide_mul_u32(offset_y, pixel_y);

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

    let depth_limit = params.depth_limit;
    let iter_count = wide_mandelbrot(iterstart, depth_limit);

    // Write intermediate X and Y results to continue on the next iteration
    for (var i = 0u; i < 2 * word_count; i++) {
        intermediate[2 * index * word_count + i] = arena[2 * word_count + i];
    }

    iterations[index] = iter_count;
}

// ===== Bignum =====

// Override variables aren't available yet, temporary bandaid
// Tracking issue: https://github.com/gfx-rs/wgpu/issues/4484
const word_count: u32 = 8;

const arena_size: u32 = word_count * 7;
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

// Mutates `left` by writing the result of multiplication of `left` and `right` to it. Returns
// the handle to `left`
//
// NOTE: Only intended to multiply positive numbers
// NOTE: Wraps on overflow
fn wide_mul_u32(left: NumView, right: u32) -> NumView {
    var carry = 0u;
    for (var idx = left.idx; idx < left.idx + word_count; idx++) {
        let res = carrying_mul(arena[idx], right, carry);
        arena[idx] = res.x;
        carry = res.y;
    }
    return left;
}

// Mutates `out` by writing the result of squaring of `num` to it. Returns the handle to `out`
//
// NOTE: Overflow is UB
fn wide_square(num: NumView, out: NumView) -> NumView {
    for (var idx = out.idx; idx < out.idx + word_count; idx++) {
        arena[idx] = 0u;
    }

    let numneg = sign(wide_floor(num)) == -1;
    if numneg {
        wide_neg(num);
    }

    for (var i = i32(word_count) / 2 - 1; i < i32(word_count); i++) {
        let target_idx = 2 * i + 1 - i32(word_count);
        let ni = arena[num.idx + u32(i)];

        var prod = carrying_mul(ni, ni, 0u);
        if target_idx >= 0 {
            let res = carrying_add(arena[out.idx + u32(target_idx)], prod.x, 0u);
            arena[out.idx + u32(target_idx)] = res.x;
            prod.y += res.y;
        }
        wide_add_u32_at(out, u32(target_idx + 1), prod.y);
    }

    for (var i = 0; i < i32(word_count); i++) {
        // TODO: replace with max once it's added to naga
        let min_useful_index = i32(word_count) - i - 3;
        let start = select(i + 1, min_useful_index, i + 1 < min_useful_index);
        for (var j = start; j < i32(word_count); j++) {
            let target_idx = i + j + 1 - i32(word_count);

            let ni = arena[num.idx + u32(i)];
            let nj = arena[num.idx + u32(j)];
            let prod = carrying_mul(ni, nj, 0u);
            let double_lo = carrying_add(prod.x, prod.x, 0u);
            var double_hi = carrying_add(prod.y, prod.y, double_lo.y);

            if target_idx >= 0 {
                let res = carrying_add(arena[out.idx + u32(target_idx)], double_lo.x, 0u);
                arena[out.idx + u32(target_idx)] = res.x;
                double_hi.x += res.y;
            }
            if target_idx >= -1 {
                let res = carrying_add(arena[out.idx + u32(target_idx) + 1], double_hi.x, 0u);
                arena[out.idx + u32(target_idx) + 1] = res.x;
                double_hi.y += res.y;
            }
            wide_add_u32_at(out, u32(target_idx + 2), double_hi.y);
        }
    }

    if numneg {
        wide_neg(num);
    }

    return out;
}

fn wide_add_u32_at(num: NumView, offset: u32, increment: u32) {
    var inc = increment;
    for (var i = offset; i < word_count; i++) {
        var sum = carrying_add(arena[num.idx + i], inc, 0u);
        arena[num.idx + i] = sum.x;
        inc = sum.y;
        if inc == 0u { break; }
    }
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
