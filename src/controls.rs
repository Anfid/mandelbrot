use iced::{Color, Theme};
use iced_wgpu::Renderer;
use iced_widget::{button, column, container, mouse_area, scrollable, slider, text};
use iced_winit::core::alignment;
use iced_winit::core::{Element, Length};
use iced_winit::runtime::{Command, Program};
use winit::event_loop::EventLoopProxy;

use crate::UserEvent;

/// Iced Program responsible for control panel UI
#[derive(Debug)]
pub struct Controls {
    /// Main event loop proxy to send events
    event_loop_proxy: EventLoopProxy<UserEvent>,
    /// Determines if control panel is displayed or hidden
    settings_open: bool,
    /// Fractal view scale factor
    scale_factor: f64,
    /// Indicates if pointer is interacting with control panel UI
    pointer_captured: bool,
}

impl Controls {
    /// Creates a new cotrol panel instance
    pub fn new(event_loop_proxy: EventLoopProxy<UserEvent>, scale_factor: f64) -> Controls {
        Controls {
            event_loop_proxy,
            settings_open: false,
            pointer_captured: false,
            scale_factor,
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
    ScaleChanged(f64),
}

impl Program for Controls {
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
            Message::ScaleChanged(scale) => {
                self.scale_factor = scale;
                self.event_loop_proxy
                    .send_event(UserEvent::ViewScaleFactorChanged(self.scale_factor))
                    .expect("Event loop closed")
            }
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

impl Controls {
    fn settings_view(&self) -> Element<Message, Theme, Renderer> {
        let content = container(
            column![
                text(format!("Scale: {:.2}", self.scale_factor)),
                slider(
                    1.0..=10.0,
                    self.scale_factor,
                    |scale| Message::ScaleChanged(scale)
                )
            ]
            .spacing(10),
        )
        .padding(10)
        .max_width(300)
        .align_x(alignment::Horizontal::Left);

        scrollable(content).height(Length::Fill).into()
    }
}
