pub struct Timer {
    internal: TimerImpl,
}

impl Timer {
    pub fn start() -> Self {
        Self {
            internal: TimerImpl::start(),
        }
    }

    pub fn stop(self) -> f64 {
        self.internal.stop()
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct TimerImpl(std::time::Instant);

#[cfg(not(target_arch = "wasm32"))]
impl TimerImpl {
    fn start() -> Self {
        TimerImpl(std::time::Instant::now())
    }

    fn stop(self) -> f64 {
        (std::time::Instant::now() - self.0).as_secs_f64() * 1000.0
    }
}

#[cfg(target_arch = "wasm32")]
struct TimerImpl(f64);

#[cfg(target_arch = "wasm32")]
impl TimerImpl {
    fn start() -> Self {
        TimerImpl(js_sys::Date::now())
    }

    fn stop(self) -> f64 {
        js_sys::Date::now() - self.0
    }
}
