#![feature(bigint_helper_methods)]

use iced_winit::core as iced_core;
use iced_winit::runtime as iced_runtime;
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
mod overlay;
mod primitives;
mod timer;
mod view_state;

use crate::gpu::GpuContext;
use crate::primitives::{Dimensions, Point};
use crate::view_state::ViewState;

const MAX_DEPTH: u32 = u32::MAX;

#[derive(Debug, Default)]
struct InputState {
    modifiers: winit::keyboard::ModifiersState,
    pointer: Option<Point>,
    grab: HashSet<DeviceId>,
}

#[derive(Debug)]
enum UserEvent {
    WorkDone,
    RenderNeedsPolling,
    ViewScaleFactorChanged(f64),
    PositionReset,
    PrecisionChanged(usize),
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

    let default_precision = 10;

    let mut view_state = {
        let window_size = window.inner_size();
        ViewState::default(
            Dimensions::new_nonzero(window_size.width, window_size.height),
            window.scale_factor(),
            default_precision,
        )
    };

    let mut input_state = InputState::default();

    let mut gpu_context = match GpuContext::new(
        &window,
        view_state.dimensions(),
        view_state.scale_factor(),
        view_state.coords(),
        30.0,
    )
    .await
    {
        Ok(context) => context,
        Err(e) => {
            log::error!("Unable to initialize a GPU context: {:?}", e);

            #[cfg(target_arch = "wasm32")]
            {
                let root = web_sys::window()
                    .unwrap()
                    .document()
                    .unwrap()
                    .get_element_by_id("root")
                    .unwrap();
                root.set_inner_html(&format!(
                    "<h3>This browser doesn't have WebGPU support yet</h3>\n<p>detailed error: {}</p>",
                    e
                ));
            }

            return;
        }
    };

    let controls = overlay::Overlay::new(event_loop_proxy.clone(), window.scale_factor());
    let mut clipboard = iced_winit::Clipboard::unconnected();
    let mut ui_state = iced_runtime::program::State::new(
        controls,
        gpu_context.viewport().logical_size(),
        &mut gpu_context.ui_renderer,
        &mut gpu_context.ui_debug,
    );

    let mut theme = iced::Theme::Light;

    event_loop
        .run(|event, elwt| {
            match event {
                Event::WindowEvent { event, .. } => {
                    match &event {
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
                        WindowEvent::ModifiersChanged(modifiers) => {
                            input_state.modifiers = modifiers.state();
                        }
                        WindowEvent::Resized(new_size) => {
                            let dimensions =
                                Dimensions::new_nonzero(new_size.width, new_size.height);
                            view_state.set_dimensions(dimensions);
                            gpu_context.resize_and_update_params(
                                dimensions,
                                view_state.scale_factor(),
                                view_state.coords().clone(),
                            );

                            window.request_redraw();
                        }
                        WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                            gpu_context.rescale_ui(*scale_factor);
                            window.request_redraw();
                        }
                        WindowEvent::TouchpadMagnify { delta, .. } => {
                            view_state.zoom_with_anchor(*delta as f32, input_state.pointer);
                            gpu_context.update_params(view_state.coords().clone());
                            window.request_redraw();
                        }
                        WindowEvent::MouseWheel {
                            delta: scroll_delta,
                            ..
                        } => {
                            let delta = match scroll_delta {
                                MouseScrollDelta::LineDelta(_, delta) => *delta,
                                MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition {
                                    x: _,
                                    y: delta,
                                }) => (*delta / 500.0) as f32,
                            };
                            if delta != 0.0 {
                                view_state.zoom_with_anchor(delta, input_state.pointer);
                                gpu_context.update_params(view_state.coords().clone());
                                window.request_redraw();
                            }
                        }
                        WindowEvent::MouseInput {
                            device_id,
                            state: ElementState::Pressed,
                            button: MouseButton::Left,
                        } => {
                            if !ui_state.program().is_pointer_captured() {
                                input_state.grab.insert(*device_id);
                            }
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
                                    if delta_x.abs() >= 0.05 || delta_y.abs() >= 0.05 {
                                        view_state.move_by_screen_delta(delta_x, delta_y);
                                        gpu_context.update_params(view_state.coords().clone());

                                        window.request_redraw();
                                    }
                                }
                            }
                            input_state.pointer = Some(new_position);
                        }
                        WindowEvent::CursorLeft { device_id } => {
                            input_state.grab.remove(device_id);
                            input_state.pointer = None;
                        }
                        WindowEvent::MouseInput {
                            device_id,
                            state: ElementState::Released,
                            button: MouseButton::Left,
                        } => {
                            input_state.grab.remove(device_id);
                        }
                        WindowEvent::Touch(_touch) => {
                            todo!("Handle touch")
                        }
                        WindowEvent::ThemeChanged(os_theme) => match os_theme {
                            winit::window::Theme::Light => theme = iced::theme::Theme::Light,
                            winit::window::Theme::Dark => theme = iced::theme::Theme::Dark,
                        },
                        WindowEvent::RedrawRequested => match gpu_context.render() {
                            Ok(()) => {
                                // Update the mouse cursor
                                window.set_cursor_icon(iced_winit::conversion::mouse_interaction(
                                    ui_state.mouse_interaction(),
                                ));
                                event_loop_proxy
                                    .send_event(UserEvent::RenderNeedsPolling)
                                    .expect("Event loop closed");
                            }
                            Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                            Err(e) => log::warn!("Render error: {:?}", e),
                        },
                        _ => {}
                    };

                    if let Some(iced_event) = iced_winit::conversion::window_event(
                        iced_core::window::Id::MAIN,
                        event,
                        window.scale_factor(),
                        input_state.modifiers,
                    ) {
                        ui_state.queue_event(iced_event);
                    }

                    // Update iced if any events are pending
                    if !ui_state.is_queue_empty() {
                        let _ = ui_state.update(
                            gpu_context.viewport().logical_size(),
                            input_state
                                .pointer
                                .map(|p| {
                                    iced_winit::conversion::cursor_position(
                                        winit::dpi::PhysicalPosition::new(p.x as f64, p.y as f64),
                                        window.scale_factor(),
                                    )
                                })
                                .map(iced_core::mouse::Cursor::Available)
                                .unwrap_or(iced_core::mouse::Cursor::Unavailable),
                            &mut gpu_context.ui_renderer,
                            &theme,
                            &iced_core::renderer::Style {
                                text_color: theme.palette().text,
                            },
                            &mut clipboard,
                            &mut gpu_context.ui_debug,
                        );

                        window.request_redraw();
                    }
                }
                Event::UserEvent(event) => match event {
                    UserEvent::ViewScaleFactorChanged(scale_factor) => {
                        view_state.set_scale_factor(scale_factor);
                        gpu_context.resize_and_update_params(
                            view_state.dimensions(),
                            view_state.scale_factor(),
                            view_state.coords().clone(),
                        );
                        window.request_redraw();
                    }

                    UserEvent::PositionReset => {
                        view_state.reset();
                        gpu_context.update_params(view_state.coords().clone());
                        window.request_redraw();
                    }

                    UserEvent::PrecisionChanged(precision) => {
                        view_state.set_precision(precision);
                        gpu_context.update_params(view_state.coords().clone());
                        window.request_redraw();
                    }

                    UserEvent::WorkDone => {
                        gpu_context.on_work_done();
                        window.request_redraw()
                    }
                    UserEvent::RenderNeedsPolling => match gpu_context.poll() {
                        wgpu::MaintainResult::SubmissionQueueEmpty => {
                            event_loop_proxy
                                .send_event(UserEvent::WorkDone)
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
            };
        })
        .unwrap();
}
