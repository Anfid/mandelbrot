use crate::timer::Timer;
use std::cmp::max;
use std::collections::BTreeMap;

pub struct FpsBalancer {
    /// Iteration limit for full redraws
    present_iterations: BTreeMap<usize, u32>,

    /// Iteration limit for next iterations
    pub iteration_iterations: u32,

    /// FPS target that balancer tries to reach
    target_ms_per_iter: f64,

    calibration_state: Option<(usize, u32)>,
    present_iteration_limit: BTreeMap<usize, u32>,

    /// Frame timer
    timer: Option<FrameTimer>,
}

impl FpsBalancer {
    pub const UNCALIBRATED_LIMIT: u32 = 15;
    pub const PRESENTATION_DEFAULT: u32 = 10;

    pub fn new(target_fps: f64) -> Self {
        let target_ms_per_iter = 1000.0 / target_fps;
        Self {
            target_ms_per_iter,
            present_iterations: Default::default(),
            iteration_iterations: Self::PRESENTATION_DEFAULT,
            calibration_state: None,
            present_iteration_limit: Default::default(),
            timer: None,
        }
    }

    pub fn reset(&mut self) {
        self.present_iterations = Default::default();
        self.iteration_iterations = Self::PRESENTATION_DEFAULT;
        self.calibration_state = None;
        self.present_iteration_limit = Default::default();
        self.timer = None;
    }

    pub fn start_presentation_frame(&mut self, number_size: usize) {
        self.timer = Some(FrameTimer::Presentation(TimerInfo {
            timer: Timer::start(),
            number_size,
        }));
    }

    pub fn start_calibration_frame(&mut self, number_size: usize) -> u32 {
        let (size, lim) = self.calibration_state.get_or_insert((number_size, 5));
        if *size != number_size {
            *size = number_size;
            *lim = 5;
        }
        self.timer = Some(FrameTimer::Calibration(TimerInfo {
            timer: Timer::start(),
            number_size,
        }));
        *lim
    }

    pub fn start_iteration_frame(&mut self) {
        self.timer = Some(FrameTimer::Iteration(Timer::start()));
    }

    pub fn is_calibrated(&self, number_size: usize) -> bool {
        self.present_iteration_limit.contains_key(&number_size)
    }

    pub fn end_frame(&mut self) {
        match self.timer.take() {
            Some(FrameTimer::Presentation(TimerInfo { timer, number_size })) => {
                let frame_time = timer.stop();

                let present_iterations = &self
                    .present_iterations
                    .get(&number_size)
                    .copied()
                    .unwrap_or(Self::PRESENTATION_DEFAULT);

                let correction = iteration_correction(self.target_ms_per_iter, frame_time);

                let iterations = ((*present_iterations as f64 * correction).round() as u32)
                    .min(self.present_iteration_limit(number_size));

                self.present_iterations.insert(number_size, iterations);
                self.iteration_iterations = self.present_iterations(number_size);
                log::info!("present: {}", self.iteration_iterations);
            }
            Some(FrameTimer::Calibration(TimerInfo { timer, number_size })) => {
                if let Some((calibration_number_size, limit)) = self.calibration_state.take() {
                    if number_size != calibration_number_size {
                        return;
                    }
                    let frame_time = timer.stop();

                    let correction = iteration_correction(self.target_ms_per_iter, frame_time);
                    let limit = (limit as f64 * correction).round() as u32;

                    if 0.98 < correction && correction < 1.02 {
                        log::info!("present limit: max {} at {number_size} words", limit * 3);
                        self.present_iteration_limit.insert(number_size, limit);
                    } else {
                        self.calibration_state = Some((number_size, limit));
                    }
                }
            }
            Some(FrameTimer::Iteration(t)) => {
                let correction = iteration_correction(self.target_ms_per_iter, t.stop());
                let new_iteration_count =
                    (self.iteration_iterations as f64 * correction).round() as u32;
                // At least 1 iteration per frame
                self.iteration_iterations = max(new_iteration_count, 1);
                log::debug!("iteration: {}", self.iteration_iterations);
            }
            None => {}
        }
    }

    pub fn present_iterations(&self, number_size: usize) -> u32 {
        self.present_iterations
            .get(&number_size)
            .copied()
            .unwrap_or(Self::PRESENTATION_DEFAULT)
            .max(1)
            .min(self.present_iteration_limit(number_size))
    }

    fn present_iteration_limit(&self, number_size: usize) -> u32 {
        self.present_iteration_limit
            .get(&number_size)
            .copied()
            .map(|l| l * 3)
            .unwrap_or(Self::UNCALIBRATED_LIMIT)
            .max(1)
    }
}

enum FrameTimer {
    Presentation(TimerInfo),
    Calibration(TimerInfo),
    Iteration(Timer),
}

struct TimerInfo {
    timer: Timer,
    number_size: usize,
}

fn iteration_correction(target_ms: f64, actual_ms: f64) -> f64 {
    if actual_ms > 0.0 {
        // Smooth multiplier by reducing it to 50%
        (target_ms / actual_ms - 1.0) * 0.5 + 1.0
    } else {
        // Avoid infinity, double the iteration count if frame time is faster than timer detects
        2.0
    }
}
