use iced_wgpu::core as iced_core;
use iced_winit::runtime as iced_runtime;
use std::borrow::Cow;
use std::cmp::min;
use thiserror::Error;
use winit::window::Window;

use crate::fps_balancer::FpsBalancer;
use crate::primitives::{Coordinates, Dimensions, ScaledDimensions};
use crate::MAX_DEPTH;

mod compute;
mod render;

use self::compute::{ComputeBindings, ComputeParams};
use self::render::{FragmentParams, RenderBindings};

const COMPUTE_SHADER_TEMPLATE: &str = include_str!("compute.wgsl");

pub struct GpuContext<'w> {
    device: wgpu::Device,
    queue: wgpu::Queue,

    config: wgpu::SurfaceConfiguration,
    surface: wgpu::Surface<'w>,

    pub ui_renderer: iced_wgpu::Renderer,
    pub ui_debug: iced_runtime::Debug,
    viewport: iced_wgpu::graphics::Viewport,

    compute_bind_group_layout: wgpu::BindGroupLayout,
    compute_pipeline: wgpu::ComputePipeline,
    compute_bindings: ComputeBindings,
    calibration_bindings: ComputeBindings,

    render_bind_group_layout: wgpu::BindGroupLayout,
    render_pipeline: wgpu::RenderPipeline,
    render_bindings: RenderBindings,

    state: State,
    params: ParamsState,
}

struct State {
    /// Amount of iterations for this invocation
    fps_balancer: FpsBalancer,
    task: Option<Task>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Task {
    Render,
    Calibration,
}

/// Fractal calculation parameters that CPU is responsible to keep track of
struct ParamsState {
    /// Current calculated depth
    depth: u32,

    /// View scale factor
    scale: f64,

    /// The amount of words in each number in comupte shader
    word_count: usize,
    /// View dimensions, scaled by view_scale
    scaled_dimensions: ScaledDimensions,

    /// Parameter update to be applied on the next iteration start
    update: Option<ParamsUpdate>,
}

enum ParamsUpdate {
    Move {
        coords: Coordinates,
    },
    Resize {
        dimensions: Dimensions,
        scale: f64,
        coords: Coordinates,
    },
}

fn calibration_coords(size: usize) -> Coordinates {
    use crate::float::WideFloat;

    // Coordinates of the top left corner of the biggest 16:10 rectangle that can be inscribed in the main cardioid
    // Thanks to Koitz for calculating them for me
    Coordinates {
        x: WideFloat::from_f32(-0.6827560061104002, size).unwrap(),
        y: WideFloat::from_f32(-0.2914862451646308, size).unwrap(),
        step: WideFloat::from_raw(
            std::iter::once(Coordinates::PRECISION_THRESHOLD)
                .chain(std::iter::repeat(0).take(size - 1))
                .collect(),
        ),
    }
}

#[derive(Debug, Error)]
pub enum ContextCreationError {
    #[error("Create surface error: {0}")]
    SurfaceCreation(#[from] wgpu::CreateSurfaceError),
    #[error("Surface not supported by this adapter")]
    SurfaceUnsupported,
    #[error("Request adapter error")]
    AdapterRequest,
    #[error("Request device error: {0}")]
    DeviceRequest(#[from] wgpu::RequestDeviceError),
}

impl<'w> GpuContext<'w> {
    pub async fn new(
        window: &'w Window,
        dimensions: Dimensions,
        scale: f64,
        coords: &Coordinates,
        fps: f64,
    ) -> Result<Self, ContextCreationError> {
        let scaled_dimensions = dimensions.scale_to(scale);

        let viewport = iced_wgpu::graphics::Viewport::with_physical_size(
            iced_core::Size::new(dimensions.width, dimensions.height),
            scale,
        );

        let state = State {
            fps_balancer: FpsBalancer::new(fps),
            task: None,
        };

        let params = ParamsState {
            depth: 0,
            scale,
            word_count: coords.size(),
            scaled_dimensions,
            update: None,
        };

        // GPU handle
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            // Should opt-out of WebGL here as it doesn't support compute shaders, but
            // wgpu::Instance::request_adapter panics on unsupported platforms otherwise
            backends: wgpu::Backends::all(),
            flags: wgpu::InstanceFlags::default(),
            dx12_shader_compiler: wgpu::Dx12Compiler::default(),
            gles_minor_version: wgpu::Gles3MinorVersion::default(),
        });

        let surface = instance.create_surface(window)?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or(ContextCreationError::AdapterRequest)?;

        let mut device_limits = wgpu::Limits::default().using_resolution(adapter.limits());

        // TODO: Save the limit and use it for buffer sizing
        device_limits.max_storage_buffer_binding_size =
            adapter.limits().max_storage_buffer_binding_size;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    required_limits: device_limits,
                    label: None,
                },
                None, // Trace path
            )
            .await?;

        let compute_shader_src = COMPUTE_SHADER_TEMPLATE.replace(
            "const word_count: u32 = 8;",
            &format!("const word_count: u32 = {};", params.word_count),
        );

        let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(compute_shader_src)),
        });
        let render_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("render.wgsl"))),
        });

        let compute_bind_group_layout =
            device.create_bind_group_layout(&ComputeBindings::bind_group_layout_desc());

        let present_iterations = state.fps_balancer.present_iterations(params.word_count);
        let compute_bindings = ComputeBindings::new(
            &device,
            &compute_bind_group_layout,
            scaled_dimensions,
            coords.size(),
        )
        .write(
            &queue,
            &ComputeParams::new(scaled_dimensions, coords, present_iterations),
        );
        let calibration_bindings = ComputeBindings::new(
            &device,
            &compute_bind_group_layout,
            scaled_dimensions,
            coords.size(),
        )
        .write(
            &queue,
            &ComputeParams::new(
                scaled_dimensions,
                &calibration_coords(coords.size()),
                present_iterations,
            ),
        );

        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Compute PipelineLayout"),
                bind_group_layouts: &[&compute_bind_group_layout],
                push_constant_ranges: &[],
            });
        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Compute Pipeline"),
            layout: Some(&compute_pipeline_layout),
            module: &compute_shader,
            entry_point: "main",
        });

        let render_bind_group_layout =
            device.create_bind_group_layout(&RenderBindings::bind_group_layout_desc());

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline"),
                bind_group_layouts: &[&render_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_bindings =
            RenderBindings::new(&device, &render_bind_group_layout, scaled_dimensions).write(
                &queue,
                FragmentParams {
                    size: scaled_dimensions,
                    depth: 0,
                },
            );

        let swapchain_capabilities = surface.get_capabilities(&adapter);
        let swapchain_format = swapchain_capabilities.formats[0];

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &render_shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &render_shader,
                entry_point: "fs_main",
                targets: &[Some(swapchain_format.into())],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Front),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let mut config = surface
            .get_default_config(&adapter, dimensions.width, dimensions.height)
            .ok_or(ContextCreationError::SurfaceUnsupported)?;
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);

        let ui_renderer = iced_wgpu::Renderer::new(
            iced_wgpu::Backend::new(
                &device,
                &queue,
                iced_wgpu::Settings::default(),
                swapchain_format,
            ),
            iced::Font::default(),
            iced::Pixels(16.0),
        );
        let ui_debug = iced_runtime::Debug::new();

        Ok(Self {
            device,
            queue,
            config,
            surface,
            ui_renderer,
            ui_debug,
            viewport,
            compute_bind_group_layout,
            compute_pipeline,
            compute_bindings,
            calibration_bindings,
            render_bind_group_layout,
            render_pipeline,
            render_bindings,
            state,
            params,
        })
    }

    pub fn rescale_ui(&mut self, window_scale: f64) {
        self.viewport = iced_wgpu::graphics::Viewport::with_physical_size(
            self.viewport.physical_size(),
            window_scale,
        );
    }

    pub fn resize_and_update_params(
        &mut self,
        dimensions: Dimensions,
        scale: f64,
        coords: Coordinates,
    ) {
        self.viewport = iced_wgpu::graphics::Viewport::with_physical_size(
            iced_core::Size::new(dimensions.width, dimensions.height),
            self.viewport.scale_factor(),
        );
        self.params.update = Some(ParamsUpdate::Resize {
            dimensions,
            scale,
            coords,
        });
    }

    pub fn update_params(&mut self, new_coords: Coordinates) {
        match &mut self.params.update {
            Some(ParamsUpdate::Resize { coords, .. }) => {
                *coords = new_coords;
            }
            update => *update = Some(ParamsUpdate::Move { coords: new_coords }),
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        if self.state.task.is_some() {
            return Ok(());
        }
        self.state.task = Some(Task::Render);
        if self.params.update.is_none() {
            self.state.fps_balancer.start_iteration_frame();
        } else {
            self.state
                .fps_balancer
                .start_presentation_frame(self.params.word_count);
        }

        self.apply_updates();

        let dimensions = self.dimensions().scale_to(self.params.scale);

        let frame = self
            .surface
            .get_current_texture()
            .expect("Failed to acquire next swap chain texture");
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let iterations = if self.params.depth == 0 {
            self.state
                .fps_balancer
                .present_iterations(self.params.word_count)
        } else {
            self.state.fps_balancer.iteration_iterations
        };
        self.params.depth = min(self.params.depth.saturating_add(iterations), MAX_DEPTH);
        self.render_bindings.write(
            &self.queue,
            FragmentParams {
                size: dimensions,
                depth: self.params.depth,
            },
        );

        let mut command_encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        command_encoder.push_debug_group("Compute");
        {
            let mut cpass = command_encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.compute_pipeline);
            cpass.set_bind_group(0, &self.compute_bindings.bind_group, &[]);
            cpass.dispatch_workgroups(dimensions.aligned_width(64) / 64, dimensions.height, 1);
        }
        command_encoder.pop_debug_group();

        command_encoder.copy_buffer_to_texture(
            wgpu::ImageCopyBuffer {
                buffer: &self.compute_bindings.result_buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(self.render_bindings.texture.size().width * 4),
                    rows_per_image: None,
                },
            },
            self.render_bindings.texture.as_image_copy(),
            self.render_bindings.texture.size(),
        );

        command_encoder.push_debug_group("Render");
        {
            let mut rpass = command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rpass.set_pipeline(&self.render_pipeline);
            rpass.set_bind_group(0, &self.render_bindings.bind_group, &[]);
            rpass.draw(0..4, 0..1);
        }
        command_encoder.pop_debug_group();

        // Render iced UI on top
        self.ui_renderer.with_primitives(|backend, primitive| {
            backend.present(
                &self.device,
                &self.queue,
                &mut command_encoder,
                None,
                frame.texture.format(),
                &view,
                primitive,
                &self.viewport,
                &self.ui_debug.overlay(),
            );
        });

        // submit will accept anything that implements IntoIter
        self.queue.submit(Some(command_encoder.finish()));
        frame.present();

        Ok(())
    }

    pub fn poll(&self) -> wgpu::MaintainResult {
        self.device.poll(wgpu::Maintain::Poll)
    }

    pub fn on_work_done(&mut self) {
        self.state.fps_balancer.end_frame();

        if self.state.task == Some(Task::Render)
            && !self
                .state
                .fps_balancer
                .is_calibrated(self.params.word_count)
        {
            self.calibrate();
            self.state.task = Some(Task::Calibration);
        } else {
            self.state.task = None;
        }
    }

    pub fn dimensions(&self) -> Dimensions {
        Dimensions {
            width: self.config.width,
            height: self.config.height,
        }
    }

    pub fn viewport(&self) -> &iced_wgpu::graphics::Viewport {
        &self.viewport
    }

    fn calibrate(&mut self) {
        let iter_count = self
            .state
            .fps_balancer
            .start_calibration_frame(self.params.word_count);

        let dimensions = self.dimensions().scale_to(self.params.scale);

        let mut command_encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        self.calibration_bindings
            .write_iterate(&mut self.queue, iter_count);

        command_encoder.push_debug_group("Calibrate");
        {
            let mut cpass = command_encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.compute_pipeline);
            cpass.set_bind_group(0, &self.calibration_bindings.bind_group, &[]);
            cpass.dispatch_workgroups(dimensions.aligned_width(64) / 64, dimensions.height, 1);
        }
        command_encoder.pop_debug_group();

        // submit will accept anything that implements IntoIter
        self.queue.submit(Some(command_encoder.finish()));
    }

    fn apply_updates(&mut self) {
        match self.params.update.take() {
            Some(ParamsUpdate::Move { coords }) => {
                // Reset calculated depth
                self.params.depth = 0;

                // NOTE: Not extracted into a dedicated function since it's a temporary solution while override
                // variables are not supported in wgpu
                if coords.size() != self.params.word_count {
                    log::info!("Changing number word count to {}", coords.size());
                    self.params.word_count = coords.size();
                    let compute_shader_src = COMPUTE_SHADER_TEMPLATE.replace(
                        "const word_count: u32 = 8;",
                        &format!("const word_count: u32 = {};", self.params.word_count),
                    );
                    let compute_shader =
                        self.device
                            .create_shader_module(wgpu::ShaderModuleDescriptor {
                                label: None,
                                source: wgpu::ShaderSource::Wgsl(Cow::Owned(compute_shader_src)),
                            });
                    self.compute_pipeline =
                        self.device
                            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                                label: Some("Compute Pipeline"),
                                layout: Some(&self.device.create_pipeline_layout(
                                    &wgpu::PipelineLayoutDescriptor {
                                        label: Some("Compute PipelineLayout"),
                                        bind_group_layouts: &[&self.compute_bind_group_layout],
                                        push_constant_ranges: &[],
                                    },
                                )),
                                module: &compute_shader,
                                entry_point: "main",
                            });

                    // Resize compute shader bindings
                    self.compute_bindings = ComputeBindings::new(
                        &self.device,
                        &self.compute_bind_group_layout,
                        self.params.scaled_dimensions,
                        coords.size(),
                    )
                    .write(
                        &self.queue,
                        &ComputeParams::new(
                            self.params.scaled_dimensions,
                            &coords,
                            self.state
                                .fps_balancer
                                .present_iterations(self.params.word_count),
                        ),
                    );
                    if !self
                        .state
                        .fps_balancer
                        .is_calibrated(self.params.word_count)
                    {
                        self.calibration_bindings = ComputeBindings::new(
                            &self.device,
                            &self.compute_bind_group_layout,
                            self.params.scaled_dimensions,
                            coords.size(),
                        )
                        .write(
                            &self.queue,
                            &ComputeParams::new(
                                self.params.scaled_dimensions,
                                &calibration_coords(coords.size()),
                                FpsBalancer::UNCALIBRATED_LIMIT,
                            ),
                        );
                    }
                } else {
                    self.compute_bindings.write(
                        &self.queue,
                        &ComputeParams::new(
                            self.dimensions().scale_to(self.params.scale),
                            &coords,
                            self.state
                                .fps_balancer
                                .present_iterations(self.params.word_count),
                        ),
                    );
                }
            }
            Some(ParamsUpdate::Resize {
                dimensions,
                scale,
                coords,
            }) => {
                // Reset calculated depth
                self.params.depth = 0;

                // Reset fps balancer
                self.state.fps_balancer.reset();

                // Update window scale
                self.params.scale = scale;

                // Reconfigure the surface
                self.config.width = dimensions.width;
                self.config.height = dimensions.height;
                self.surface.configure(&self.device, &self.config);

                let scaled_dimensions = dimensions.scale_to(scale);
                self.params.scaled_dimensions = scaled_dimensions;

                // NOTE: Not extracted into a dedicated function since it's a temporary solution while override
                // variables are not supported in wgpu
                if coords.size() != self.params.word_count {
                    log::info!("Changing number word count to {}", coords.size());
                    self.params.word_count = coords.size();
                    let compute_shader_src = COMPUTE_SHADER_TEMPLATE.replace(
                        "const word_count: u32 = 8;",
                        &format!("const word_count: u32 = {};", self.params.word_count),
                    );
                    let compute_shader =
                        self.device
                            .create_shader_module(wgpu::ShaderModuleDescriptor {
                                label: None,
                                source: wgpu::ShaderSource::Wgsl(Cow::Owned(compute_shader_src)),
                            });
                    self.compute_pipeline =
                        self.device
                            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                                label: Some("Compute Pipeline"),
                                layout: Some(&self.device.create_pipeline_layout(
                                    &wgpu::PipelineLayoutDescriptor {
                                        label: Some("Compute PipelineLayout"),
                                        bind_group_layouts: &[&self.compute_bind_group_layout],
                                        push_constant_ranges: &[],
                                    },
                                )),
                                module: &compute_shader,
                                entry_point: "main",
                            });
                }

                // Resize compute shader bindings
                self.compute_bindings = ComputeBindings::new(
                    &self.device,
                    &self.compute_bind_group_layout,
                    scaled_dimensions,
                    coords.size(),
                )
                .write(
                    &self.queue,
                    &ComputeParams::new(
                        scaled_dimensions,
                        &coords,
                        self.state
                            .fps_balancer
                            .present_iterations(self.params.word_count),
                    ),
                );

                // Update calibration bindings
                self.calibration_bindings = ComputeBindings::new(
                    &self.device,
                    &self.compute_bind_group_layout,
                    scaled_dimensions,
                    coords.size(),
                )
                .write(
                    &self.queue,
                    &ComputeParams::new(
                        self.params.scaled_dimensions,
                        &calibration_coords(coords.size()),
                        FpsBalancer::UNCALIBRATED_LIMIT,
                    ),
                );

                // Resize render shader bindings
                self.render_bindings = RenderBindings::new(
                    &self.device,
                    &self.render_bind_group_layout,
                    scaled_dimensions,
                )
                .write(
                    &self.queue,
                    FragmentParams {
                        size: scaled_dimensions,
                        depth: 0,
                    },
                );
            }
            None => {
                let iterations = if self.params.depth == 0 {
                    self.state
                        .fps_balancer
                        .present_iterations(self.params.word_count)
                } else {
                    self.state.fps_balancer.iteration_iterations
                };
                self.compute_bindings.write_iterate(&self.queue, iterations);
            }
        }
    }
}
