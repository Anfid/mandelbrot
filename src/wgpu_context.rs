use crate::primitives::Dimensions;
use std::borrow::Cow;
use winit::window::Window;

pub struct WgpuContext<'w> {
    device: wgpu::Device,
    queue: wgpu::Queue,
    render: RendererContext<'w>,
    compute: ComputeContext,
}

pub struct RendererContext<'w> {
    config: wgpu::SurfaceConfiguration,
    surface: wgpu::Surface<'w>,
    render_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    bind_group_layout: wgpu::BindGroupLayout,
    texture: wgpu::Texture,
}

pub struct ComputeContext {
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    params_buffer: wgpu::Buffer,
    result_buffer: wgpu::Buffer,
}

impl<'w> WgpuContext<'w> {
    pub async fn new(window: &'w Window, params: &[f32]) -> Self {
        let mut window_size = window.inner_size();
        window_size.width = window_size.width.max(1);
        window_size.height = window_size.height.max(1);

        // The instance is a handle to our GPU
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

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    // WebGL doesn't support all of wgpu's features, so if
                    // we're building for the web, we'll have to disable some.
                    required_limits: if cfg!(target_arch = "wasm32") {
                        wgpu::Limits::downlevel_webgl2_defaults()
                    } else {
                        wgpu::Limits::default()
                    }
                    .using_resolution(adapter.limits()),
                    label: None,
                },
                None, // Trace path
            )
            .await
            .unwrap();

        // Load the shaders from disk
        let visual_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("visual.wgsl"))),
        });
        let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("compute.wgsl"))),
        });

        let compute_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            // Going to have this be None just to be safe.
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            // Going to have this be None just to be safe.
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let aligned_width =
            (window_size.width as u64 / 64 + (window_size.width as u64 % 64 != 0) as u64) * 64;

        let (params_buffer, result_buffer, compute_bind_group) = create_compute_bind_group(
            &device,
            &queue,
            &compute_bind_group_layout,
            4 * aligned_width * window_size.height as wgpu::BufferAddress,
            params,
        );

        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&compute_bind_group_layout],
                push_constant_ranges: &[],
            });
        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: Some(&compute_pipeline_layout),
            module: &compute_shader,
            entry_point: "main",
        });

        let render_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                }],
            });

        let texture_extent = wgpu::Extent3d {
            width: aligned_width as u32,
            height: window_size.height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Render itercount"),
            size: texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let render_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &render_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture_view),
            }],
            label: None,
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&render_bind_group_layout],
                push_constant_ranges: &[],
            });

        let swapchain_capabilities = surface.get_capabilities(&adapter);
        let swapchain_format = swapchain_capabilities.formats[0];

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &visual_shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &visual_shader,
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
            .get_default_config(&adapter, window_size.width, window_size.height)
            .unwrap();
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);

        let render = RendererContext {
            config,
            surface,
            render_pipeline,
            bind_group: render_bind_group,
            bind_group_layout: render_bind_group_layout,
            texture,
        };

        let compute = ComputeContext {
            bind_group_layout: compute_bind_group_layout,
            pipeline: compute_pipeline,
            bind_group: compute_bind_group,
            params_buffer,
            result_buffer,
        };

        Self {
            device,
            queue,
            render,
            compute,
        }
    }

    pub fn resize_and_update_params(&mut self, dimensions: Dimensions, params: &[f32]) {
        self.render.set_dimensions(&self.device, dimensions);

        let aligned_width =
            (dimensions.width as u64 / 64 + (dimensions.width as u64 % 64 != 0) as u64) * 64;

        let extent = wgpu::Extent3d {
            width: aligned_width as u32,
            height: dimensions.height,
            depth_or_array_layers: 1,
        };

        (
            self.compute.params_buffer,
            self.compute.result_buffer,
            self.compute.bind_group,
        ) = create_compute_bind_group(
            &self.device,
            &self.queue,
            &self.compute.bind_group_layout,
            4 * aligned_width * dimensions.height as wgpu::BufferAddress,
            params,
        );

        self.render.texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let texture_view = self
            .render
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.render.bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.render.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture_view),
            }],
            label: None,
        });
    }

    pub fn update_params(&mut self, params: &[f32]) {
        let params = bytemuck::cast_slice(params);
        self.queue
            .write_buffer(&self.compute.params_buffer, 0, &params);
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let frame = self
            .render
            .surface
            .get_current_texture()
            .expect("Failed to acquire next swap chain texture");
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut command_encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        let dimensions = self.render.dimensions();
        command_encoder.push_debug_group("Compute mandelbrot");
        {
            let mut cpass = command_encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.compute.pipeline);
            cpass.set_bind_group(0, &self.compute.bind_group, &[]);
            cpass.dispatch_workgroups(dimensions.width, dimensions.height, 1);
        }
        command_encoder.pop_debug_group();

        command_encoder.copy_buffer_to_texture(
            wgpu::ImageCopyBuffer {
                buffer: &self.compute.result_buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(self.render.texture.size().width * 4),
                    rows_per_image: None,
                },
            },
            self.render.texture.as_image_copy(),
            self.render.texture.size(),
        );

        command_encoder.push_debug_group("Render mandelbrot");
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
            rpass.set_pipeline(&self.render.render_pipeline);
            rpass.set_bind_group(0, &self.render.bind_group, &[]);
            rpass.draw(0..4, 0..1);
        }
        command_encoder.pop_debug_group();

        // submit will accept anything that implements IntoIter
        self.queue.submit(Some(command_encoder.finish()));
        frame.present();

        Ok(())
    }

    pub fn dimensions(&self) -> Dimensions {
        self.render.dimensions()
    }
}

impl<'w> RendererContext<'w> {
    fn dimensions(&self) -> Dimensions {
        Dimensions {
            width: self.config.width,
            height: self.config.height,
        }
    }

    fn set_dimensions(&mut self, device: &wgpu::Device, dimensions: Dimensions) {
        self.config.width = dimensions.width;
        self.config.height = dimensions.height;

        self.surface.configure(device, &self.config);
    }
}

fn create_compute_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    size: wgpu::BufferAddress,
    params: &[f32],
) -> (wgpu::Buffer, wgpu::Buffer, wgpu::BindGroup) {
    // Buffer to pass input parameters to the GPU
    let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Compute in"),
        size: 8 * size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&params_buffer, 0, bytemuck::cast_slice(params));

    // Buffer with result produced by the GPU
    let result_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Compute out"),
        size: 4 * size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let compute_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: result_buffer.as_entire_binding(),
            },
        ],
    });
    (params_buffer, result_buffer, compute_bind_group)
}
