use std::cmp::min;

use crate::float::WideFloat;
use bytemuck::{Pod, Zeroable};

#[derive(Debug, Clone, Copy)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

impl Dimensions {
    pub fn new_nonzero(width: u32, height: u32) -> Self {
        Self {
            width: width.max(1),
            height: height.max(1),
        }
    }

    pub fn scale_to(&self, scale: f64) -> ScaledDimensions {
        ScaledDimensions {
            width: ((self.width as f64) / scale).round() as u32,
            height: (self.height as f64 / scale).round() as u32,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct ScaledDimensions {
    pub width: u32,
    pub height: u32,
}

impl ScaledDimensions {
    pub fn aligned_width(&self, alignment: u32) -> u32 {
        self.width.div_ceil(alignment) * alignment
    }

    pub fn shortest_side(&self) -> u32 {
        self.width.min(self.height)
    }
}

#[derive(Debug, Clone)]
pub struct Coordinates {
    pub x: WideFloat,
    pub y: WideFloat,
    pub step: WideFloat,
}

impl Coordinates {
    pub fn new(x: f32, y: f32, step: f32) -> Self {
        let x = WideFloat::from_f32(x, 2).expect("Invalid x");
        let y = WideFloat::from_f32(y, 2).expect("Invalid y");
        let step = WideFloat::from_f32(step, 2).expect("Invalid step");
        Coordinates { x, y, step }
    }

    pub fn move_by_delta(&mut self, dx: f32, dy: f32) {
        self.x -=
            &(&WideFloat::from_f32(dx, self.size()).expect("Invalid move delta") * &self.step);
        self.y -=
            &(&WideFloat::from_f32(dy, self.size()).expect("Invalid move delta") * &self.step);
    }

    pub fn rescale_to_point(&mut self, mul: f32, x: i32, y: i32, max_limit: f32) {
        if mul < 1.0 && self.step.requires_precision(1000) {
            self.increase_precision();
        } else if mul > 1.0 && self.step.excess_precision(1000) {
            self.decrease_precision();
        }

        let wide_x = WideFloat::from_i32(x, self.size());
        let wide_y = WideFloat::from_i32(y, self.size());
        let wide_mul = WideFloat::from_f32(mul, self.size()).unwrap();

        let mut new_step = &self.step * &wide_mul;

        // Limit zoom out
        if self.size() == 2 {
            let wide_max_limit =
                WideFloat::from_f32(max_limit, self.size()).expect("Invalid max limit");
            new_step = min(wide_max_limit, new_step);
        }

        let dx = &(self.step.clone() - &new_step) * &wide_x;
        let dy = &(self.step.clone() - &new_step) * &wide_y;

        self.step = new_step;
        self.x += &dx;
        self.y += &dy;
    }

    pub fn size(&self) -> usize {
        self.step.word_count()
    }

    fn increase_precision(&mut self) {
        self.x.increase_precision();
        self.y.increase_precision();
        self.step.increase_precision();
    }

    fn decrease_precision(&mut self) {
        self.x.decrease_precision();
        self.y.decrease_precision();
        self.step.decrease_precision();
    }
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy, Pod, Zeroable)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
