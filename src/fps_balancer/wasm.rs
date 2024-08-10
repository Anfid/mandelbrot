// TODO: Write a proper wasm implementation once it becomes possible to time the work done on the GPU in the web.
//
// Currently there's no way to time completion of GPU work in the browser.
// Queue::on_submitted_work_done is unimplemented
// Device::poll is a noop, as devices are polled by the browser
pub struct FpsBalancer {
    pub iteration_iterations: u32,
}

impl FpsBalancer {
    pub const UNCALIBRATED_LIMIT: u32 = 20;
    pub const PRESENTATION_DEFAULT: u32 = 20;

    pub fn new(_: f64) -> Self {
        Self {
            iteration_iterations: Self::PRESENTATION_DEFAULT,
        }
    }

    pub fn reset(&self) {}

    pub fn start_presentation_frame(&self, _: usize) {}

    pub fn start_calibration_frame(&self, _: usize) -> u32 {
        Self::PRESENTATION_DEFAULT
    }

    pub fn start_iteration_frame(&self) {}

    pub fn is_calibrated(&self, _: usize) -> bool {
        true
    }

    pub fn end_frame(&self) {}

    pub fn present_iterations(&self, _: usize) -> u32 {
        Self::PRESENTATION_DEFAULT
    }
}
