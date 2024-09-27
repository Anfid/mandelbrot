use iced::{Color, Theme};
use iced_wgpu::Renderer;
use iced_widget::{button, column, container, mouse_area, scrollable, slider, text};
use iced_winit::core::alignment;
use iced_winit::core::{Element, Length};
use iced_winit::runtime::{Command, Program};
use winit::event_loop::EventLoopProxy;

use crate::gpu::ColorParams;
use crate::UserEvent;

/// Iced Program responsible for control panel UI
#[derive(Debug)]
pub struct Overlay {
    /// Main event loop proxy to send events
    event_loop_proxy: EventLoopProxy<UserEvent>,
    /// Indicates if pointer is interacting with control panel UI
    pointer_captured: bool,
    /// Determines if control panel is displayed or hidden
    settings_open: bool,
    /// Max calculation depth
    max_depth: u32,
    /// Square root of fractal view scale factor. Square to get an actual scale factor value.
    /// Stored as sqrt to allow exponential scaling in the linear slider
    scale_factor_sqrt: f64,
    /// Color parameters
    color_params: ColorParams,
    /// Amount of extra 32 bit words of precision
    precision_words: u32,
    /// Statistics and information
    info: Info,
}

impl Overlay {
    /// Creates a new cotrol panel instance
    pub fn new(
        event_loop_proxy: EventLoopProxy<UserEvent>,
        scale_factor: f64,
        max_depth: u32,
        color_params: ColorParams,
    ) -> Overlay {
        Overlay {
            event_loop_proxy,
            pointer_captured: false,
            settings_open: false,
            max_depth,
            scale_factor_sqrt: scale_factor.sqrt(),
            color_params,
            precision_words: 0,
            info: Default::default(),
        }
    }

    /// Returns true if pointer is currently interacting with control panel UI
    pub fn is_pointer_captured(&self) -> bool {
        self.pointer_captured
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    ToggleSettings,
    CapturePointer(bool),
    MaxDepthChanged(u32),
    ScaleChanged(f64),
    ColorChanged(ColorParams),
    PositionReset,
    PrecisionChanged(u32),
    InfoUpdated(Info),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Info {
    pub depth: u32,
}

impl Program for Overlay {
    type Theme = Theme;
    type Message = Message;
    type Renderer = Renderer;

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::ToggleSettings => {
                self.settings_open = !self.settings_open;
            }
            Message::CapturePointer(status) => {
                self.pointer_captured = status;
            }
            Message::MaxDepthChanged(depth) => {
                self.max_depth = depth;
                self.event_loop_proxy
                    .send_event(UserEvent::MaxDepthChanged(depth))
                    .expect("Event loop closed");
            }
            Message::ScaleChanged(scale) => {
                self.scale_factor_sqrt = scale;
                self.event_loop_proxy
                    .send_event(UserEvent::ViewScaleFactorChanged(scale * scale))
                    .expect("Event loop closed")
            }
            Message::ColorChanged(colors) => {
                self.color_params = colors;
                self.event_loop_proxy
                    .send_event(UserEvent::ColorChanged(colors))
                    .expect("Event loop closed")
            }
            Message::PositionReset => self
                .event_loop_proxy
                .send_event(UserEvent::PositionReset)
                .expect("Event loop closed"),
            Message::PrecisionChanged(precision) => {
                self.precision_words = precision;
                self.event_loop_proxy
                    .send_event(UserEvent::PrecisionChanged(self.precision_bits()))
                    .expect("Event loop closed")
            }
            Message::InfoUpdated(info) => self.info = info,
        }

        Command::none()
    }

    fn view(&self) -> Element<Message, Theme, Renderer> {
        let toggle_button_label = if self.settings_open { "X" } else { "=" };
        let toggle_button = button(toggle_button_label).on_press(Message::ToggleSettings);

        let interface = if self.settings_open {
            column![toggle_button, self.settings_view()].max_width(300)
        } else {
            column![toggle_button]
        };

        mouse_area(
            container(interface)
                .width(Length::Shrink)
                .style(iced::theme::Container::from(|theme: &iced::Theme| {
                    iced_widget::container::Appearance {
                        background: Some(
                            iced::Color {
                                a: 0.6,
                                ..theme.palette().background
                            }
                            .into(),
                        ),
                        shadow: iced::Shadow {
                            color: Color::BLACK,
                            offset: Default::default(),
                            blur_radius: 10.0,
                        },
                        ..Default::default()
                    }
                })),
        )
        .on_enter(Message::CapturePointer(true))
        .on_exit(Message::CapturePointer(false))
        .into()
    }
}

impl Overlay {
    fn settings_view(&self) -> Element<Message, Theme, Renderer> {
        let content = container(
            column![
                text(format!("Depth: {}/{}", self.info.depth, self.max_depth)),
                slider(
                    1..=(u32::MAX.ilog2() + 1) * 16,
                    max_depth_to_slider(self.max_depth),
                    |depth| { Message::MaxDepthChanged(slider_to_max_depth(depth)) },
                ),
                text(format!(
                    "Scale: {:.2}",
                    self.scale_factor_sqrt * self.scale_factor_sqrt
                )),
                slider(1.0..=30.0_f64.sqrt(), self.scale_factor_sqrt, |scale| {
                    Message::ScaleChanged(scale)
                })
                .step(0.01),
                text(format!(
                    "Color exponentiation: {}",
                    match self.color_params.depth_exp {
                        0.0 => String::from("log2(n)"),
                        1.0 => String::from("n"),
                        pow => format!("{:.3}√n", pow.recip()),
                    }
                )),
                slider(0.0..=1.0, self.color_params.depth_exp, |depth_exp| {
                    let depth_exp = if depth_exp < 0.15 { 0.0 } else { depth_exp };
                    Message::ColorChanged(ColorParams {
                        depth_exp,
                        ..self.color_params
                    })
                })
                .step(0.001),
                text("Color shift"),
                slider(
                    0.0..=2.0 * std::f32::consts::PI,
                    self.color_params.shift,
                    |shift| {
                        Message::ColorChanged(ColorParams {
                            shift,
                            ..self.color_params
                        })
                    }
                )
                .step(0.01),
                text("Color buffer"),
                slider(2..=100, self.color_params.buffer, |buffer| {
                    Message::ColorChanged(ColorParams {
                        buffer,
                        ..self.color_params
                    })
                }),
                text("Color cutoff"),
                slider(0.0..=2.0, self.color_params.cutoff, |cutoff| {
                    Message::ColorChanged(ColorParams {
                        cutoff,
                        ..self.color_params
                    })
                })
                .step(0.01),
                text(format!("Precision: {}", self.precision_bits())),
                slider(0..=4, self.precision_words, |p| {
                    Message::PrecisionChanged(p)
                })
                .step(1u32),
                button("Reset position").on_press(Message::PositionReset),
            ]
            .spacing(10),
        )
        .padding(10)
        .max_width(300)
        .align_x(alignment::Horizontal::Left);

        scrollable(content).height(Length::Fill).into()
    }

    fn precision_bits(&self) -> usize {
        if self.precision_words == 0 {
            10
        } else {
            self.precision_words as usize * 32
        }
    }
}

fn slider_to_max_depth(v: u32) -> u32 {
    let p = v / 16;
    let f = v % 16;
    let part_size = (1u64 << p) as f32 / 16.0;
    2u32.saturating_pow(p)
        .saturating_add((part_size * f as f32) as u32)
}

fn max_depth_to_slider(v: u32) -> u32 {
    let base = v.ilog2();
    let part_size = 1 << base;
    let part = v ^ part_size;
    (base * 16).saturating_add((part as f32 / (part_size as f32 / 16 as f32)).ceil() as u32)
}
