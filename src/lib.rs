#![feature(bigint_helper_methods)]

use fractal::Fractal;
use std::collections::HashSet;

use crate::wide_float::WideFloat;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::WindowBuilder,
};

mod fractal;
mod renderer;
mod timer;
mod wide_float;

#[derive(Debug, Default, Clone)]
struct PrecisePoint {
    pub x: WideFloat<5>,
    pub y: WideFloat<5>,
}

#[derive(Debug, Default, Clone, Copy)]
struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone)]
struct PreciseViewState {
    center: PrecisePoint,
    point_size: WideFloat<5>,
}

#[derive(Debug, Clone)]
struct FastViewState {
    center: Point,
    scale: u64,
}

#[derive(Debug, Clone)]
enum ViewState {
    Fast(FastViewState),
    Precise(PreciseViewState),
}

//impl Default for ViewState {
//    fn default() -> Self {
//        Self::Fast(FastViewState {
//            center: Point { x: 0.0, y: 0.0 },
//            scale: 200,
//        })
//    }
//}

impl Default for ViewState {
    fn default() -> Self {
        Self::Precise(PreciseViewState {
            center: PrecisePoint {
                x: WideFloat::<5>::default(),
                y: WideFloat::<5>::default(),
            },
            point_size: WideFloat::<5>::try_from(4.0 / 1000.0).unwrap(),
        })
    }
}

impl ViewState {
    pub fn rescale_to_point(&mut self, delta: f64, point: Option<Point>, w: u32, h: u32) {
        let point = point.unwrap_or_default();
        match self {
            ViewState::Fast(ref mut fstate) => {
                let cx = fstate.center.x + (point.x - w as f64 / 2.0) / fstate.scale as f64;
                let cy = fstate.center.y + (point.y - h as f64 / 2.0) / fstate.scale as f64;
                let mul = if delta > 0.0 {
                    1.0 + delta
                } else {
                    1.0 / (1.0 - delta)
                };
                fstate.scale = (mul * fstate.scale as f64).round().abs() as u64;
                let dx =
                    cx - (&fstate.center.x + (point.x - w as f64 / 2.0) / (fstate.scale as f64));
                let dy =
                    cy - (&fstate.center.y + (point.y - h as f64 / 2.0) / (fstate.scale as f64));
                fstate.center.x += dx;
                fstate.center.y -= dy;
                log::info!(
                    "x: {}, y: {}, scale: {}",
                    fstate.center.x,
                    fstate.center.y,
                    fstate.scale
                );
            }
            ViewState::Precise(ref mut pstate) => {
                let cx = &WideFloat::<5>::from(point.x as i64 - w as i64 / 2) * &pstate.point_size
                    + &pstate.center.x;
                let cy = &WideFloat::<5>::from(point.y as i64 - h as i64 / 2) * &pstate.point_size
                    + &pstate.center.y;
                let mul = if delta > 0.0 {
                    WideFloat::<5>::try_from(1.0 / (1.0 + delta))
                        .expect("Invalid magnify delta value")
                } else {
                    WideFloat::<5>::try_from(1.0 - delta).expect("Invalid magnify delta value")
                };
                pstate.point_size *= &mul;
                let dx = cx
                    - &(&WideFloat::<5>::from(point.x as i64 - w as i64 / 2) * &pstate.point_size
                        + &pstate.center.x);
                let dy = cy
                    - &(&WideFloat::<5>::from(point.y as i64 - h as i64 / 2) * &pstate.point_size
                        + &pstate.center.y);
                pstate.center.x += &dx;
                pstate.center.y -= &dy;
            }
        }
    }

    fn move_by_screen_delta(&mut self, delta_x: f64, delta_y: f64) {
        match self {
            ViewState::Fast(ref mut fstate) => {
                fstate.center = Point {
                    x: fstate.center.x - (delta_x / fstate.scale as f64),
                    y: fstate.center.y + (delta_y / fstate.scale as f64),
                }
            }
            ViewState::Precise(ref mut pstate) => {
                pstate.center.x -= &(&WideFloat::<5>::try_from(delta_x)
                    .expect("Invalid pointer position")
                    * &pstate.point_size);
                pstate.center.y += &(&WideFloat::<5>::try_from(delta_y)
                    .expect("Invalid pointer position")
                    * &pstate.point_size);
            }
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
    //let default_texels = f(window_size.width.max(1), window_size.height.max(1), &state);
    let mut fractal_state =
        Fractal::new(window_size.width.max(1), window_size.height.max(1), &state);
    let default_texels = fractal_state.get_texels();
    let mut renderer_state = renderer::RendererState::new(&window, default_texels).await;

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
                    let width = new_size.width.max(1);
                    let height = new_size.height.max(1);
                    fractal_state = Fractal::new(width, height, &state);
                    let texels = fractal_state.get_texels();
                    renderer_state.resize_and_update_texture(width, height, texels);

                    window.request_redraw();
                }
                WindowEvent::TouchpadMagnify { delta, .. } => {
                    state.rescale_to_point(
                        *delta,
                        input_state.pointer,
                        renderer_state.config.width,
                        renderer_state.config.height,
                    );
                    fractal_state = Fractal::new(
                        renderer_state.config.width,
                        renderer_state.config.height,
                        &state,
                    );
                    let texels = fractal_state.get_texels();
                    renderer_state.update_texture(texels);
                    window.request_redraw();
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    match delta {
                        MouseScrollDelta::LineDelta(_, delta) => {
                            state.rescale_to_point(
                                *delta as f64,
                                input_state.pointer,
                                renderer_state.config.width,
                                renderer_state.config.height,
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
                                renderer_state.config.width,
                                renderer_state.config.height,
                            );
                        }
                    };
                    fractal_state = Fractal::new(
                        renderer_state.config.width,
                        renderer_state.config.height,
                        &state,
                    );
                    let texels = fractal_state.get_texels();
                    renderer_state.update_texture(texels);
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
                            fractal_state = Fractal::new(
                                renderer_state.config.width,
                                renderer_state.config.height,
                                &state,
                            );
                            let texels = fractal_state.get_texels();
                            renderer_state.update_texture(texels);

                            // Windows doesn't respect redraw request and requires force render
                            renderer_state.render();
                            //window.request_redraw();
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
                WindowEvent::RedrawRequested => match renderer_state.render() {
                    Ok(()) => {}
                    Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                    Err(e) => log::warn!("Render error: {:?}", e),
                },
                _ => {}
            },
            Event::AboutToWait => {
                if !fractal_state.is_final() {
                    fractal_state.iterate();
                    let texels = fractal_state.get_texels();
                    renderer_state.update_texture(texels);
                    window.request_redraw();
                }
            }
            _ => {}
        })
        .unwrap();
}
