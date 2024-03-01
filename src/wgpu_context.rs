use crate::{
    float::WideFloat,
    primitives::{Dimensions, PrecisePoint},
};
use bytemuck::{Pod, Zeroable};
use std::borrow::Cow;
use winit::window::Window;

const WORD_COUNT: u32 = 5;

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
    params_buffer: wgpu::Buffer,
    texture: wgpu::Texture,
}

pub struct ComputeContext {
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    params_buffer: wgpu::Buffer,
    result_buffer: wgpu::Buffer,
}

#[derive(Debug, Clone)]
pub struct ComputeParams {
    pub size: Dimensions,
    pub frame_iterations: u32,
    pub top_left: PrecisePoint,
    pub point_step: WideFloat<5>,
}

impl ComputeParams {
    fn encode(&self) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(self.size_hint() as usize);
        buffer.extend_from_slice(&bytemuck::cast::<_, [u8; 8]>(self.size));
        buffer.extend_from_slice(&bytemuck::cast::<_, [u8; 4]>(self.frame_iterations));
        buffer.extend_from_slice(bytemuck::cast_slice(&self.top_left.x.0));
        buffer.extend_from_slice(bytemuck::cast_slice(&self.top_left.y.0));
        buffer.extend_from_slice(bytemuck::cast_slice(&self.point_step.0));
        buffer
    }

    fn size_hint(&self) -> u32 {
        WORD_COUNT * 12 + 16
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct FragmentParams {
    pub size: Dimensions,
}

impl<'w> WgpuContext<'w> {
    pub async fn new(window: &'w Window, params: &ComputeParams) -> Self {
        let window_size = window.inner_size();
        let view_dimensions = Dimensions {
            width: window_size.width.max(1),
            height: window_size.height.max(1),
        };
        let aligned_dimensions = view_dimensions.align_width_to(64);

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
                    // WebGL doesn't support all of wgpu's features, so if
                    // we're building for the web, we'll have to disable some.
                    required_limits: device_limits,
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

        let (compute_params_buffer, result_buffer, compute_bind_group) = create_compute_bind_group(
            &device,
            &queue,
            &compute_bind_group_layout,
            aligned_dimensions,
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
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            sample_type: wgpu::TextureSampleType::Uint,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
            });

        let texture = device.create_texture(&itercount_texture_desc(aligned_dimensions.into()));
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let fragment_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("FragmentParams"),
            size: std::mem::size_of::<FragmentParams>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bytes: [u8; std::mem::size_of::<FragmentParams>()] = bytemuck::cast(FragmentParams {
            size: view_dimensions,
        });
        queue.write_buffer(&fragment_params_buffer, 0, &bytes);

        let render_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &render_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: fragment_params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
            ],
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
            .get_default_config(&adapter, view_dimensions.width, view_dimensions.height)
            .unwrap();
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);

        let render = RendererContext {
            config,
            surface,
            render_pipeline,
            bind_group: render_bind_group,
            bind_group_layout: render_bind_group_layout,
            params_buffer: fragment_params_buffer,
            texture,
        };

        let compute = ComputeContext {
            bind_group_layout: compute_bind_group_layout,
            pipeline: compute_pipeline,
            bind_group: compute_bind_group,
            params_buffer: compute_params_buffer,
            result_buffer,
        };

        Self {
            device,
            queue,
            render,
            compute,
        }
    }

    pub fn resize_and_update_params(&mut self, dimensions: Dimensions, params: &ComputeParams) {
        self.render.set_dimensions(&self.device, dimensions);

        let aligned_dimensions = dimensions.align_width_to(64);

        (
            self.compute.params_buffer,
            self.compute.result_buffer,
            self.compute.bind_group,
        ) = create_compute_bind_group(
            &self.device,
            &self.queue,
            &self.compute.bind_group_layout,
            aligned_dimensions,
            params,
        );

        let bytes: [u8; std::mem::size_of::<FragmentParams>()] =
            bytemuck::cast(FragmentParams { size: dimensions });
        self.queue
            .write_buffer(&self.render.params_buffer, 0, &bytes);

        self.render.texture = self
            .device
            .create_texture(&itercount_texture_desc(aligned_dimensions.into()));
        let texture_view = self
            .render
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.render.bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.render.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.render.params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
            ],
            label: None,
        });
    }

    pub fn update_params(&mut self, params: &ComputeParams) {
        self.queue
            .write_buffer(&self.compute.params_buffer, 0, &params.encode());
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

        let dimensions = self.dimensions();
        command_encoder.push_debug_group("Compute");
        {
            let mut cpass = command_encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.compute.pipeline);
            cpass.set_bind_group(0, &self.compute.bind_group, &[]);
            cpass.dispatch_workgroups(dimensions.width / 64, dimensions.height, 1);
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
    size: Dimensions,
    params: &ComputeParams,
) -> (wgpu::Buffer, wgpu::Buffer, wgpu::BindGroup) {
    // Buffer to pass input parameters to the GPU
    let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ComputeParams"),
        size: 24.max(std::mem::size_of::<ComputeParams>() as u64),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&params_buffer, 0, &params.encode());

    // Buffer with result produced by the GPU
    let result_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ItercountBuffer"),
        size: (4 * size.width * size.height) as u64,
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

fn itercount_texture_desc(aligned_extent: wgpu::Extent3d) -> wgpu::TextureDescriptor<'static> {
    wgpu::TextureDescriptor {
        label: Some("ItercountTexture"),
        size: aligned_extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R32Uint,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    }
}
