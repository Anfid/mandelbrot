use std::hash::{Hash, Hasher};

use crate::{Point, PrecisePoint, ViewState};
use malachite::{num::arithmetic::traits::Square, Rational};
use rayon::iter::{IntoParallelIterator, ParallelIterator};

pub const DEPTH_LIMIT: u32 = u32::MAX;
pub const FPS: u32 = 30;

pub enum Fractal {
    Fast(Vec<FastPointStatus>, u32),
    Precise(Vec<PrecisePointStatus>),
}

impl Fractal {
    pub fn new(width: u32, height: u32, state: &ViewState) -> Self {
        let mut buffer = Vec::with_capacity(width as usize * height as usize);
        let half_w = width as f64 / 2.0;
        let half_h = height as f64 / 2.0;
        let ViewState::Fast(fstate) = state else {
            todo!()
        };
        for px_y in 0..height {
            for px_x in 0..width {
                let x = fstate.center.x + (px_x as f64 - half_w) / fstate.scale as f64;
                let y = fstate.center.y + (px_y as f64 - half_h) / fstate.scale as f64;
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
        let mut fractal = Self::Fast(buffer, 20);
        fractal.iterate();
        fractal
    }

    pub fn iterate(&mut self) {
        match self {
            Self::Fast(ref mut buffer, ref mut iteration_count) => {
                use std::time::{Duration, Instant};
                let start = Instant::now();
                buffer
                    .into_par_iter()
                    .update(|fstatus| match fstatus {
                        FastPointStatus::Done(i) => **fstatus = FastPointStatus::Done(*i),
                        FastPointStatus::Iteration(i, fstate) => {
                            let mut x2 = fstate.x * fstate.x;
                            let mut y2 = fstate.y * fstate.y;
                            let old_i = *i;
                            while *i < DEPTH_LIMIT && x2 + y2 < 4.0 {
                                fstate.y = 2.0 * fstate.x * fstate.y + fstate.coords.y;
                                fstate.x = x2 - y2 + fstate.coords.x;
                                *i += 1;

                                if *i - old_i >= *iteration_count {
                                    return;
                                }
                                x2 = fstate.x * fstate.x;
                                y2 = fstate.y * fstate.y;
                            }
                            **fstatus = FastPointStatus::Done(*i);
                        }
                    })
                    .for_each(|_| ());
                let duration = Instant::now() - start;
                let ratio =
                    Duration::from_millis(1000 / FPS as u64).as_secs_f64() / duration.as_secs_f64();
                *iteration_count = std::cmp::min((*iteration_count as f64 * ratio) as u32, 1000);
            }
            Self::Precise(ref mut buffer) => {
                buffer
                    .into_par_iter()
                    .update(|pstatus| match pstatus {
                        PrecisePointStatus::Done(i) => **pstatus = PrecisePointStatus::Done(*i),
                        PrecisePointStatus::Iteration(_, ref mut i, ref mut pstate) => {
                            let x_sq = (&pstate.x).square();
                            let y_sq = (&pstate.y).square();

                            if *i < DEPTH_LIMIT && &x_sq + &y_sq < 4 {
                                // Periodicity check
                                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                                (&pstate.x, &pstate.y).hash(&mut hasher);
                                let hash = hasher.finish();
                                if pstate.periodicity.contains(&hash) {
                                    **pstatus = PrecisePointStatus::Done(DEPTH_LIMIT);
                                    return;
                                } else {
                                    pstate.periodicity.push(hash)
                                }

                                pstate.y.mutate_numerator(|n| *n <<= 1);
                                pstate.y = &pstate.x * &pstate.y + &pstate.coords.y;
                                pstate.x = x_sq - y_sq + &pstate.coords.x;
                                //x.approximate_assign(&self.precision_lim);
                                //y.approximate_assign(&self.precision_lim);

                                *i += 1;
                            } else {
                                **pstatus = PrecisePointStatus::Done(*i);
                            }
                        }
                        PrecisePointStatus::Approx(_, _) => todo!(),
                    })
                    .for_each(|_| ());
            }
        }
    }

    pub fn get_texels(&self) -> Vec<u8> {
        match self {
            Fractal::Fast(buffer, _) => buffer
                .iter()
                .map(|s| match s {
                    FastPointStatus::Done(i) => *i as u32,
                    FastPointStatus::Iteration(_, _) => DEPTH_LIMIT,
                })
                .map(u32::to_le_bytes)
                .flatten()
                .collect(),
            Fractal::Precise(buffer) => buffer
                .iter()
                .map(|s| match s {
                    PrecisePointStatus::Approx(_, _) => DEPTH_LIMIT,
                    PrecisePointStatus::Iteration(a, i, _) => {
                        if *a == u8::MAX {
                            DEPTH_LIMIT
                        } else {
                            std::cmp::max(*a as u32, *i)
                        }
                    }
                    PrecisePointStatus::Done(i) => *i,
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
            Fractal::Precise(buffer) => buffer
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
    Iteration(u8, u32, PrecisePointState),
    Approx(u8, FastPointState),
}

#[derive(Debug, Clone)]
pub struct PrecisePointState {
    coords: PrecisePoint,
    x: Rational,
    y: Rational,
    periodicity: Vec<u64>,
}
