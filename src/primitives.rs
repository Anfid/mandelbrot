use crate::float::WideFloat;
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
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

    pub fn align_width_to(&self, alignment: u32) -> Self {
        Dimensions {
            width: self.width.div_ceil(alignment) * alignment,
            height: self.height,
        }
    }

    pub fn shortest_side(&self) -> u32 {
        self.width.min(self.height)
    }
}

impl Into<wgpu::Extent3d> for Dimensions {
    fn into(self) -> wgpu::Extent3d {
        wgpu::Extent3d {
            width: self.width,
            height: self.height,
            depth_or_array_layers: 1,
        }
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
