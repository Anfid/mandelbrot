use crate::float::WideFloat;
use crate::WORD_COUNT;
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

#[derive(Debug, Default, Clone)]
pub struct PrecisePoint {
    pub x: WideFloat<WORD_COUNT>,
    pub y: WideFloat<WORD_COUNT>,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy, Pod, Zeroable)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
