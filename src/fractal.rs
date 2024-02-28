use crate::float::WideFloat;
use crate::primitives::Dimensions;
use crate::timer::Timer;
use crate::{Point, PrecisePoint, ViewState};
#[cfg(not(target_arch = "wasm32"))]
use rayon::iter::{IntoParallelIterator, ParallelIterator};

pub const DEPTH_LIMIT: u32 = 1024;
pub const FPS: u32 = 30;

pub enum Fractal {
    Fast(Vec<FastPointStatus>, u32),
}

impl Fractal {
    pub fn new(dimensions: Dimensions, state: &ViewState) -> Self {
        let aligned_width =
            (dimensions.width as u64 / 64 + (dimensions.width as u64 % 64 != 0) as u64) * 64;

        let half_w = aligned_width as f64 * 0.5;
        let half_h = dimensions.height as f64 * 0.5;
        let mut buffer = Vec::with_capacity(aligned_width as usize * dimensions.height as usize);
        let scale_mul = 1.0 / state.scale as f64;
        for px_y in 0..dimensions.height {
            for px_x in 0..aligned_width {
                let x = state.center.x + (px_x as f64 - half_w) * scale_mul;
                let y = state.center.y + (px_y as f64 - half_h) * scale_mul;
                buffer.push(FastPointStatus::Iteration(
                    0,
                    FastPointState {
                        coords: Point { x, y },
                        x,
                        y,
                    },
                ));
            }
        }
        // TODO: iterate perhaps?
        //fractal.iterate();
        Self::Fast(buffer, 20)
    }

    pub fn get_params(&self) -> Vec<f32> {
        match self {
            Fractal::Fast(buffer, _) => buffer
                .into_iter()
                .map(|state| {
                    let FastPointStatus::Iteration(_, coords) = state else {
                        todo!()
                    };
                    [coords.x as f32, coords.y as f32]
                })
                .flatten()
                .collect::<Vec<_>>(),
        }
    }

    pub fn is_final(&self) -> bool {
        match self {
            Fractal::Fast(buffer, _) => {
                buffer.iter().all(|s| matches!(s, FastPointStatus::Done(_)))
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum FastPointStatus {
    Done(u32),
    Iteration(u32, FastPointState),
}

#[derive(Debug, Clone, Copy)]
pub struct FastPointState {
    coords: Point,
    x: f64,
    y: f64,
}

#[derive(Debug, Clone)]
pub enum PrecisePointStatus {
    Done(u32),
    Iteration(u32, PrecisePointState),
}

#[derive(Debug, Clone)]
pub struct PrecisePointState {
    coords: PrecisePoint,
    x: WideFloat<5>,
    y: WideFloat<5>,
}

fn iterate_fstatus(fstatus: &mut FastPointStatus, iteration_count: u32) {
    match fstatus {
        FastPointStatus::Done(i) => *fstatus = FastPointStatus::Done(*i),
        FastPointStatus::Iteration(i, fstate) => {
            let mut x2 = fstate.x * fstate.x;
            let mut y2 = fstate.y * fstate.y;
            let old_i = *i;
            while *i < DEPTH_LIMIT && x2 + y2 < 4.0 {
                fstate.y = 2.0 * fstate.x * fstate.y + fstate.coords.y;
                fstate.x = x2 - y2 + fstate.coords.x;
                *i += 1;

                if *i - old_i >= iteration_count {
                    return;
                }
                x2 = fstate.x * fstate.x;
                y2 = fstate.y * fstate.y;
            }
            *fstatus = FastPointStatus::Done(*i);
        }
    }
}

fn iterate_pstatus(pstatus: &mut PrecisePointStatus, iteration_count: u32) {
    match pstatus {
        PrecisePointStatus::Done(i) => *pstatus = PrecisePointStatus::Done(*i),
        PrecisePointStatus::Iteration(ref mut i, ref mut pstate) => {
            let mut x2 = &pstate.x * &pstate.x;
            let mut y2 = &pstate.y * &pstate.y;

            let old_i = *i;
            while *i < DEPTH_LIMIT && x2.clone() + &y2 < 4 {
                pstate.y <<= 1;
                pstate.y = &pstate.x * &pstate.y + &pstate.coords.y;
                pstate.x = x2 - &y2 + &pstate.coords.x;

                *i += 1;
                if *i - old_i >= iteration_count {
                    return;
                }

                x2 = &pstate.x * &pstate.x;
                y2 = &pstate.y * &pstate.y;
            }
            *pstatus = PrecisePointStatus::Done(*i);
        }
    }
}
