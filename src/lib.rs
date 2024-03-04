#![feature(bigint_helper_methods)]

use std::collections::HashSet;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoopBuilder},
    keyboard::{KeyCode, PhysicalKey},
    window::WindowBuilder,
};

mod float;
mod fps_balancer;
mod gpu;
mod primitives;
mod timer;

use crate::float::WideFloat;
use crate::gpu::GpuContext;
use crate::primitives::{Dimensions, Point, PrecisePoint};

const WORD_COUNT: usize = 8;
const MAX_DEPTH: u32 = u32::MAX;

#[derive(Debug, Clone)]
struct ViewState {
    top_left: PrecisePoint,
    dimensions: Dimensions,
    window_scale: f64,
    step: WideFloat<WORD_COUNT>,
}

impl ViewState {
    fn default(dimensions: Dimensions, window_scale: f64) -> Self {
        let scaled_dimensions = dimensions.scale_to(window_scale);
        let point_size =
            WideFloat::<WORD_COUNT>::try_from(4.0 / scaled_dimensions.shortest_side() as f32)
                .expect("Invalid dimensions");
        let x = &-WideFloat::<WORD_COUNT>::from(scaled_dimensions.width as i32 / 2) * &point_size;
        let y = &-WideFloat::<WORD_COUNT>::from(scaled_dimensions.height as i32 / 2) * &point_size;
        Self {
            top_left: PrecisePoint { x, y },
            dimensions,
            window_scale,
            step: point_size,
        }
    }
}

impl ViewState {
    pub fn rescale_to_point(&mut self, delta: f32, point: Option<Point>) {
        let scaled_dimensions = self.dimensions.scale_to(self.window_scale);
        let point = point.unwrap_or(Point {
            x: (self.dimensions.width / 2) as f32,
            y: (self.dimensions.height / 2) as f32,
        });
        let wide_x = WideFloat::<WORD_COUNT>::try_from(point.x / self.window_scale as f32)
            .expect("Invalid coordinates");
        let wide_y = WideFloat::<WORD_COUNT>::try_from(point.y / self.window_scale as f32)
            .expect("Invalid coordinates");
        let cx = &wide_x * &self.step + &self.top_left.x;
        let cy = &wide_y * &self.step + &self.top_left.y;

        let mul = if delta > 0.0 {
            1.0 / (1.0 + delta)
        } else {
            1.0 - delta
        };
        self.step *= &WideFloat::<WORD_COUNT>::try_from(mul).unwrap();
        // Limit zoom out
        self.step = self.step.clone().min(
            WideFloat::<WORD_COUNT>::try_from(2.0 * 4.0 / scaled_dimensions.shortest_side() as f32)
                .unwrap(),
        );
        // Limit zoom in
        self.step = WideFloat::<WORD_COUNT>::min_positive().max(self.step.clone());
        let dx = cx - &(&wide_x * &self.step + &self.top_left.x);
        let dy = cy - &(&wide_y * &self.step + &self.top_left.y);
        self.top_left.x += &dx;
        self.top_left.y += &dy;
        log::info!(
            "x: {}, y: {}, scale: {}",
            self.top_left.x.as_f32_round(),
            self.top_left.y.as_f32_round(),
            self.step.as_f32_round(),
        );
    }

    fn move_by_screen_delta(&mut self, delta_x: f32, delta_y: f32) {
        self.top_left.x -=
            &(&WideFloat::<WORD_COUNT>::try_from(delta_x / self.window_scale as f32)
                .expect("Invalid delta")
                * &self.step);
        self.top_left.y -=
            &(&WideFloat::<WORD_COUNT>::try_from(delta_y / self.window_scale as f32)
                .expect("Invalid delta")
                * &self.step);
        log::info!(
            "x: {}, y: {}",
            self.top_left.x.as_f32_round(),
            self.top_left.y.as_f32_round(),
        );
    }
}

#[derive(Debug, Default)]
struct InputState {
    pointer: Option<Point>,
    grab: HashSet<DeviceId>,
}

#[derive(Debug)]
enum UserEvent {
    RenderDone,
    RenderNeedsPolling,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub async fn run() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::init();
    }
    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");
    }

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event()
        .build()
        .unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);

    let event_loop_proxy = event_loop.create_proxy();

    #[allow(unused_mut)]
    let mut builder = WindowBuilder::new();

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        use winit::platform::web::WindowBuilderExtWebSys;
        let canvas = web_sys::window()
            .unwrap()
            .document()
            .unwrap()
            .get_element_by_id("mandelbrot-canvas")
            .unwrap()
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .unwrap();
        builder = builder.with_canvas(Some(canvas));
    }
    let window = builder.with_title("Mandelbrot").build(&event_loop).unwrap();

    let mut view_state = {
        let window_size = window.inner_size();
        ViewState::default(
            Dimensions::new_nonzero(window_size.width, window_size.height),
            window.scale_factor(),
        )
    };

    let mut input_state = InputState::default();

    let mut gpu_context = GpuContext::new(
        &window,
        view_state.dimensions,
        view_state.window_scale,
        &view_state.top_left,
        &view_state.step,
        30.0,
    )
    .await;

    // Reset to default view on screen resize until any user input
    let mut view_reset = true;

    event_loop
        .run(|event, elwt| match event {
            Event::WindowEvent { ref event, .. } => match event {
                WindowEvent::CloseRequested
                | WindowEvent::KeyboardInput {
                    event:
                        KeyEvent {
                            state: ElementState::Pressed,
                            physical_key: PhysicalKey::Code(KeyCode::Escape),
                            ..
                        },
                    ..
                } => elwt.exit(),
                WindowEvent::Resized(new_size) => {
                    let dimensions = Dimensions::new_nonzero(new_size.width, new_size.height);
                    if view_reset {
                        view_state = ViewState::default(dimensions, view_state.window_scale);
                    } else {
                        view_state.dimensions = dimensions;
                    }
                    gpu_context.resize_and_update_params(
                        dimensions,
                        view_state.window_scale,
                        view_state.top_left.clone(),
                        view_state.step.clone(),
                    );

                    window.request_redraw();
                }
                WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                    if view_reset {
                        view_state = ViewState::default(view_state.dimensions, *scale_factor);
                    } else {
                        view_state.window_scale = *scale_factor;
                    }
                    gpu_context.resize_and_update_params(
                        view_state.dimensions,
                        view_state.window_scale,
                        view_state.top_left.clone(),
                        view_state.step.clone(),
                    );

                    window.request_redraw();
                }
                WindowEvent::TouchpadMagnify { delta, .. } => {
                    view_reset = false;
                    view_state.rescale_to_point(*delta as f32, input_state.pointer);
                    gpu_context.update_params(view_state.top_left.clone(), view_state.step.clone());
                    window.request_redraw();
                }
                WindowEvent::MouseWheel {
                    delta: scroll_delta,
                    ..
                } => {
                    view_reset = false;
                    let delta = match scroll_delta {
                        MouseScrollDelta::LineDelta(_, delta) => *delta,
                        MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition {
                            x: _,
                            y: delta,
                        }) => (*delta / 500.0) as f32,
                    };
                    if delta != 0.0 {
                        view_state.rescale_to_point(delta, input_state.pointer);
                        gpu_context
                            .update_params(view_state.top_left.clone(), view_state.step.clone());
                        window.request_redraw();
                    }
                }
                WindowEvent::MouseInput {
                    device_id,
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                } => {
                    view_reset = false;
                    input_state.grab.insert(*device_id);
                }
                WindowEvent::CursorMoved {
                    device_id: _,
                    position,
                } => {
                    let new_position = Point {
                        x: position.x as f32,
                        y: position.y as f32,
                    };
                    if !input_state.grab.is_empty() {
                        if let Some(old_position) = &input_state.pointer {
                            let delta_x = new_position.x - old_position.x;
                            let delta_y = new_position.y - old_position.y;
                            view_state.move_by_screen_delta(delta_x, delta_y);
                            gpu_context.update_params(
                                view_state.top_left.clone(),
                                view_state.step.clone(),
                            );

                            window.request_redraw();
                        }
                    }
                    input_state.pointer = Some(new_position);
                }
                WindowEvent::CursorLeft { device_id }
                | WindowEvent::MouseInput {
                    device_id,
                    state: ElementState::Released,
                    button: MouseButton::Left,
                } => {
                    input_state.grab.remove(device_id);
                    input_state.pointer = None;
                }
                WindowEvent::Touch(_touch) => {
                    todo!("Handle touch")
                }
                WindowEvent::RedrawRequested => match gpu_context.render() {
                    Ok(()) => {
                        event_loop_proxy
                            .send_event(UserEvent::RenderNeedsPolling)
                            .expect("Event loop closed");
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                    Err(e) => log::warn!("Render error: {:?}", e),
                },
                _ => {}
            },
            Event::AboutToWait => {}
            Event::UserEvent(event) => match event {
                UserEvent::RenderDone => {
                    gpu_context.on_render_done();
                    window.request_redraw()
                }
                UserEvent::RenderNeedsPolling => match gpu_context.poll() {
                    wgpu::MaintainResult::SubmissionQueueEmpty => {
                        event_loop_proxy
                            .send_event(UserEvent::RenderDone)
                            .expect("Event loop closed");
                    }
                    wgpu::MaintainResult::Ok => {
                        event_loop_proxy
                            .send_event(UserEvent::RenderNeedsPolling)
                            .expect("Event loop closed");
                    }
                },
            },
            _ => {}
        })
        .unwrap();
}
