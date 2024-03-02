use std::borrow::Cow;
use winit::window::Window;

use crate::float::WideFloat;
use crate::primitives::{Dimensions, PrecisePoint};
use crate::{MAX_DEPTH, WORD_COUNT};

mod compute;
mod render;

use self::compute::{ComputeBindings, ComputeParams};
use self::render::{FragmentParams, RenderBindings};

pub struct GpuContext<'w> {
    device: wgpu::Device,
    queue: wgpu::Queue,

    config: wgpu::SurfaceConfiguration,
    surface: wgpu::Surface<'w>,

    compute_bind_group_layout: wgpu::BindGroupLayout,
    compute_pipeline: wgpu::ComputePipeline,
    compute_bindings: ComputeBindings,

    render_bind_group_layout: wgpu::BindGroupLayout,
    render_pipeline: wgpu::RenderPipeline,
    render_bindings: RenderBindings,

    state: ParamsState,
}

/// Fractal calculation parameters that CPU is responsible to keep track of
pub struct ParamsState {
    /// Current calculated depth
    depth: u32,
    /// Amount of iterations for this invocation
    iteration_limit: u32,
}

impl<'w> GpuContext<'w> {
    pub async fn new(
        window: &'w Window,
        dimensions: Dimensions,
        top_left: &PrecisePoint,
        step: &WideFloat<WORD_COUNT>,
        iteration_limit: u32,
    ) -> Self {
        // GPU handle
        let instance = wgpu::Instance::default();

        let surface = instance.create_surface(window).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let mut device_limits = if cfg!(target_arch = "wasm32") {
            wgpu::Limits::downlevel_webgl2_defaults()
        } else {
            wgpu::Limits::default()
        }
        .using_resolution(adapter.limits());

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
            .await
            .unwrap();

        // Load the shaders from disk
        let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("compute.wgsl"))),
        });
        let render_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("render.wgsl"))),
        });

        let compute_bind_group_layout =
            device.create_bind_group_layout(&ComputeBindings::bind_group_layout_desc());

        let compute_bindings =
            ComputeBindings::new(&device, &compute_bind_group_layout, dimensions).write(
                &queue,
                &ComputeParams::new(dimensions, top_left, step, iteration_limit),
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

        let render_bindings = RenderBindings::new(&device, &render_bind_group_layout, dimensions)
            .write(
                &queue,
                FragmentParams {
                    size: dimensions,
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
            .get_default_config(&adapter, dimensions.unaligned_width, dimensions.height)
            .unwrap();
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);

        Self {
            device,
            queue,
            config,
            surface,
            compute_bind_group_layout,
            compute_pipeline,
            compute_bindings,
            render_bind_group_layout,
            render_pipeline,
            render_bindings,
            state: ParamsState {
                depth: 0,
                iteration_limit,
            },
        }
    }

    pub fn resize_and_update_params(
        &mut self,
        dimensions: Dimensions,
        top_left: &PrecisePoint,
        step: &WideFloat<WORD_COUNT>,
    ) {
        // Reconfigure the surface
        self.config.width = dimensions.unaligned_width;
        self.config.height = dimensions.height;
        self.surface.configure(&self.device, &self.config);

        // Resize compute shader bindings
        self.compute_bindings =
            ComputeBindings::new(&self.device, &self.compute_bind_group_layout, dimensions).write(
                &self.queue,
                &ComputeParams::new(dimensions, top_left, step, self.state.iteration_limit),
            );

        // Reset calculated depth
        self.state.depth = 0;

        // Resize render shader bindings
        self.render_bindings =
            RenderBindings::new(&self.device, &self.render_bind_group_layout, dimensions).write(
                &self.queue,
                FragmentParams {
                    size: dimensions,
                    depth: 0,
                },
            );
    }

    pub fn update_params(&mut self, top_left: &PrecisePoint, step: &WideFloat<WORD_COUNT>) {
        self.state.depth = 0;
        self.compute_bindings.write(
            &self.queue,
            &ComputeParams::new(
                self.dimensions(),
                top_left,
                step,
                self.state.iteration_limit,
            ),
        );
    }

    pub fn iterate_on_render(&mut self, iterations: u32) {
        self.state.iteration_limit = iterations;
        self.compute_bindings.write_iterate(&self.queue, iterations);
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let frame = self
            .surface
            .get_current_texture()
            .expect("Failed to acquire next swap chain texture");
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.state.depth = (self.state.depth + self.state.iteration_limit).min(MAX_DEPTH);
        self.render_bindings.write(
            &self.queue,
            FragmentParams {
                size: self.dimensions(),
                depth: self.state.depth,
            },
        );

        let mut command_encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        let dimensions = self.dimensions();
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

        // submit will accept anything that implements IntoIter
        self.queue.submit(Some(command_encoder.finish()));
        frame.present();

        Ok(())
    }

    pub fn dimensions(&self) -> Dimensions {
        Dimensions {
            unaligned_width: self.config.width,
            height: self.config.height,
        }
    }
}
