use crate::float::WideFloat;
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Dimensions {
    pub unaligned_width: u32,
    pub height: u32,
}

impl Dimensions {
    pub fn new_nonzero(width: u32, height: u32) -> Self {
        Self {
            unaligned_width: width.max(1),
            height: height.max(1),
        }
    }

    pub fn aligned_width(&self, alignment: u32) -> u32 {
        self.unaligned_width.div_ceil(alignment) * alignment
    }

    pub fn shortest_side(&self) -> u32 {
        self.unaligned_width.min(self.height)
    }
}

#[derive(Debug, Default, Clone)]
pub struct PrecisePoint {
    pub x: WideFloat<5>,
    pub y: WideFloat<5>,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy, Pod, Zeroable)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
