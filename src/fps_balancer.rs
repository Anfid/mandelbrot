use crate::timer::Timer;

pub struct FpsBalancer {
    /// Iteration limit for full redraws
    pub present_iterations: u32,

    /// Iteration limit for next iterations
    pub iteration_iterations: u32,

    /// FPS target that balancer tries to reach
    target_ms_per_iter: f64,

    /// Frame timer
    timer: Option<FrameTimer>,
}

impl FpsBalancer {
    pub fn new(target_fps: f64) -> Self {
        let target_ms_per_iter = 1000.0 / target_fps;
        Self {
            target_ms_per_iter,
            present_iterations: 10,
            iteration_iterations: 10,
            timer: None,
        }
    }

    pub fn start_presentation_frame(&mut self) {
        self.timer = Some(FrameTimer::Presentation(Timer::start()))
    }

    pub fn start_iteration_frame(&mut self) {
        self.timer = Some(FrameTimer::Iteration(Timer::start()))
    }

    pub fn end_frame(&mut self) {
        match self.timer.take() {
            Some(FrameTimer::Presentation(t)) => {
                let iterations =
                    correct_iterations(self.present_iterations, self.target_ms_per_iter, t.stop());
                self.present_iterations = iterations;
                self.iteration_iterations = iterations;
                log::info!("present: {}", self.present_iterations);
            }
            Some(FrameTimer::Iteration(t)) => {
                self.iteration_iterations = correct_iterations(
                    self.iteration_iterations,
                    self.target_ms_per_iter,
                    t.stop(),
                );
                log::info!("iteration: {}", self.iteration_iterations);
            }
            None => {}
        }
    }
}

enum FrameTimer {
    Presentation(Timer),
    Iteration(Timer),
}

#[cfg(not(target_arch = "wasm32"))]
fn correct_iterations(iteration_count: u32, target_ms: f64, actual_ms: f64) -> u32 {
    // Avoid infinity, double the iteration count instead
    let iter_mul = if actual_ms != 0.0 {
        // Smooth multiplier by reducing it to 50%
        (target_ms / actual_ms - 1.0) * 0.5 + 1.0
    } else {
        2.0
    };

    (iteration_count as f64 * iter_mul)
        .round()
        // Min 10 iterations per frame
        .max(10.0)
        // Max 100k iterations per frame
        .min(100000.0) as u32
}

#[cfg(target_arch = "wasm32")]
fn correct_iterations(_: u32, _: f64, _: f64) -> u32 {
    // TODO: Write a proper implementation once it becomes possible to time the work done on the GPU in the web.
    //
    // Currently there's no way to time completion of GPU work in the browser.
    // Queue::on_submitted_work_done is unimplemented
    // Device::poll is a noop, as devices are polled by the browser
    20
}
