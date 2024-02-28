use crate::float::WideFloat;

#[derive(Debug, Clone, Copy)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Default, Clone)]
pub struct PrecisePoint {
    pub x: WideFloat<5>,
    pub y: WideFloat<5>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
