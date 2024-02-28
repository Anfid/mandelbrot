#![feature(bigint_helper_methods)]

use fractal::Fractal;
use std::collections::HashSet;

use crate::float::WideFloat;
use crate::primitives::{Dimensions, Point, PrecisePoint};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::WindowBuilder,
};

mod float;
mod fractal;
mod primitives;
mod timer;
mod wgpu_context;

#[derive(Debug, Clone)]
struct PreciseViewState {
    center: PrecisePoint,
    point_size: WideFloat<5>,
}

#[derive(Debug, Clone)]
struct ViewState {
    center: Point,
    scale: u64,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            center: Point { x: 0.0, y: 0.0 },
            scale: 200,
        }
    }
}

impl ViewState {
    pub fn rescale_to_point(&mut self, delta: f64, point: Option<Point>, dimensions: Dimensions) {
        let point = point.unwrap_or_default();
        let cx = self.center.x + (point.x - dimensions.width as f64 / 2.0) / self.scale as f64;
        let cy = self.center.y + (point.y - dimensions.height as f64 / 2.0) / self.scale as f64;
        let mul = if delta > 0.0 {
            1.0 + delta
        } else {
            1.0 / (1.0 - delta)
        };
        self.scale = (mul * self.scale as f64).round().abs() as u64;
        let dx =
            cx - (&self.center.x + (point.x - dimensions.width as f64 / 2.0) / (self.scale as f64));
        let dy = cy
            - (&self.center.y + (point.y - dimensions.height as f64 / 2.0) / (self.scale as f64));
        self.center.x += dx;
        self.center.y -= dy;
        log::info!(
            "x: {}, y: {}, scale: {}",
            self.center.x,
            self.center.y,
            self.scale
        );
    }

    fn move_by_screen_delta(&mut self, delta_x: f64, delta_y: f64) {
        self.center = Point {
            x: self.center.x - (delta_x / self.scale as f64),
            y: self.center.y + (delta_y / self.scale as f64),
        }
    }
}

#[derive(Debug, Default)]
struct InputState {
    pointer: Option<Point>,
    grab: HashSet<DeviceId>,
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

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);

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

    let mut state = ViewState::default();
    let mut input_state = InputState::default();

    let window_size = window.inner_size();
    let default_dimensions = Dimensions {
        width: window_size.width.max(1),
        height: window_size.height.max(1),
    };
    let mut fractal_state = Fractal::new(default_dimensions, &state);
    let starting_params = fractal_state.get_params();
    let mut wgpu_context = wgpu_context::WgpuContext::new(&window, &starting_params).await;

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
                    let dimensions = Dimensions {
                        width: new_size.width.max(1),
                        height: new_size.height.max(1),
                    };
                    fractal_state = Fractal::new(dimensions, &state);
                    let params = fractal_state.get_params();
                    wgpu_context.resize_and_update_params(dimensions, &params);

                    window.request_redraw();
                }
                WindowEvent::TouchpadMagnify { delta, .. } => {
                    state.rescale_to_point(*delta, input_state.pointer, wgpu_context.dimensions());
                    fractal_state = Fractal::new(wgpu_context.dimensions(), &state);
                    let params = fractal_state.get_params();
                    wgpu_context.update_params(&params);
                    window.request_redraw();
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    match delta {
                        MouseScrollDelta::LineDelta(_, delta) => {
                            state.rescale_to_point(
                                *delta as f64,
                                input_state.pointer,
                                wgpu_context.dimensions(),
                            );
                        }
                        MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition {
                            x: _,
                            y: delta,
                        }) => {
                            let delta = *delta / 500.0;
                            if delta == 0.0 {
                                return;
                            }
                            state.rescale_to_point(
                                delta,
                                input_state.pointer,
                                wgpu_context.dimensions(),
                            );
                        }
                    };
                    fractal_state = Fractal::new(wgpu_context.dimensions(), &state);
                    let params = fractal_state.get_params();
                    wgpu_context.update_params(&params);
                    window.request_redraw();
                }
                WindowEvent::MouseInput {
                    device_id,
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                } => {
                    input_state.grab.insert(*device_id);
                }
                WindowEvent::CursorMoved {
                    device_id: _,
                    position,
                } => {
                    let new_position = Point {
                        x: position.x,
                        y: position.y,
                    };
                    if !input_state.grab.is_empty() {
                        if let Some(old_position) = &input_state.pointer {
                            let delta_x = new_position.x - old_position.x;
                            let delta_y = new_position.y - old_position.y;
                            state.move_by_screen_delta(delta_x, delta_y);
                            fractal_state = Fractal::new(wgpu_context.dimensions(), &state);
                            let params = fractal_state.get_params();
                            wgpu_context.update_params(&params);

                            // Windows doesn't respect redraw request and requires force render
                            //renderer_context.render(&wgpu_context);
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
                WindowEvent::RedrawRequested => match wgpu_context.render() {
                    Ok(()) => {}
                    Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                    Err(e) => log::warn!("Render error: {:?}", e),
                },
                _ => {}
            },
            Event::AboutToWait => {}
            _ => {}
        })
        .unwrap();
}
