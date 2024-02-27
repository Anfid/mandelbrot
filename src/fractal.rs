use crate::timer::Timer;
use crate::wide_float::WideFloat;
use crate::{Point, PrecisePoint, ViewState};
#[cfg(not(target_arch = "wasm32"))]
use rayon::iter::{IntoParallelIterator, ParallelIterator};

pub const DEPTH_LIMIT: u32 = u32::MAX;
pub const FPS: u32 = 30;

pub enum Fractal {
    Fast(Vec<FastPointStatus>, u32),
    Precise(Vec<PrecisePointStatus>, u32),
}

impl Fractal {
    pub fn new(width: u32, height: u32, state: &ViewState) -> Self {
        let mut fractal = match state {
            ViewState::Fast(fstate) => {
                let half_w = width as f64 * 0.5;
                let half_h = height as f64 * 0.5;
                let mut buffer = Vec::with_capacity(width as usize * height as usize);
                let scale_mul = 1.0 / fstate.scale as f64;
                for px_y in 0..height {
                    for px_x in 0..width {
                        let x = fstate.center.x + (px_x as f64 - half_w) * scale_mul;
                        let y = fstate.center.y + (px_y as f64 - half_h) * scale_mul;
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
                Self::Fast(buffer, 20)
            }
            ViewState::Precise(pstate) => {
                let half_w = width as i64 / 2;
                let half_h = height as i64 / 2;
                let mut buffer = Vec::with_capacity(width as usize * height as usize);
                for px_y in 0..height {
                    for px_x in 0..width {
                        let x = &WideFloat::<5>::from(px_x as i64 - half_w) * &pstate.point_size
                            + &pstate.center.x;
                        let y = &WideFloat::<5>::from(px_y as i64 - half_h) * &pstate.point_size
                            + &pstate.center.y;
                        buffer.push(PrecisePointStatus::Iteration(
                            0,
                            PrecisePointState {
                                x: x.clone(),
                                y: y.clone(),
                                coords: PrecisePoint { x, y },
                            },
                        ));
                    }
                }
                Self::Precise(buffer, 10)
            }
        };
        fractal.iterate();
        fractal
    }

    pub fn iterate(&mut self) {
        let timer = Timer::start();
        match self {
            Self::Fast(buffer, iteration_count) => {
                // Update point statuses
                #[cfg(not(target_arch = "wasm32"))]
                {
                    buffer
                        .into_par_iter()
                        .for_each(|fstatus| iterate_fstatus(fstatus, *iteration_count));
                }
                #[cfg(target_arch = "wasm32")]
                {
                    buffer
                        .into_iter()
                        .for_each(|fstatus| iterate_fstatus(fstatus, *iteration_count));
                }
                let duration_ms = timer.stop();
                let ratio = 1000.0 / (FPS as f64 * duration_ms);
                *iteration_count = std::cmp::min((*iteration_count as f64 * ratio) as u32, 1000);
            }
            Self::Precise(buffer, iteration_count) => {
                // Update point statuses
                #[cfg(not(target_arch = "wasm32"))]
                {
                    buffer
                        .into_par_iter()
                        .for_each(|pstatus| iterate_pstatus(pstatus, *iteration_count));
                }
                #[cfg(target_arch = "wasm32")]
                {
                    buffer
                        .into_iter()
                        .for_each(|pstatus| iterate_pstatus(pstatus, *iteration_count));
                }
                let duration_ms = timer.stop();
                let ratio = 1000.0 / (FPS as f64 * duration_ms);
                *iteration_count = std::cmp::min((*iteration_count as f64 * ratio) as u32, 1000);
            }
        }
    }

    pub fn get_texels(&self) -> Vec<u8> {
        match self {
            Fractal::Fast(buffer, _) => buffer
                .iter()
                .map(|s| match s {
                    FastPointStatus::Done(i) => *i,
                    FastPointStatus::Iteration(_, _) => DEPTH_LIMIT,
                })
                .map(u32::to_le_bytes)
                .flatten()
                .collect(),
            Fractal::Precise(buffer, _) => buffer
                .iter()
                .map(|s| match s {
                    PrecisePointStatus::Done(i) => *i,
                    PrecisePointStatus::Iteration(_, _) => DEPTH_LIMIT,
                })
                .map(u32::to_le_bytes)
                .flatten()
                .collect(),
        }
    }

    pub fn is_final(&self) -> bool {
        match self {
            Fractal::Fast(buffer, _) => {
                buffer.iter().all(|s| matches!(s, FastPointStatus::Done(_)))
            }
            Fractal::Precise(buffer, _) => buffer
                .iter()
                .all(|s| matches!(s, PrecisePointStatus::Done(_))),
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
