use crate::primitives::ScaledDimensions;
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct FragmentParams {
    pub size: ScaledDimensions,
    pub depth: u32,
}

pub struct RenderBindings {
    pub(super) bind_group: wgpu::BindGroup,
    pub(super) params_buffer: wgpu::Buffer,
    pub(super) texture: wgpu::Texture,
}

impl RenderBindings {
    pub const fn bind_group_layout_desc() -> wgpu::BindGroupLayoutDescriptor<'static> {
        wgpu::BindGroupLayoutDescriptor {
            label: Some("Render BindGroupLayout"),
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
        }
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

    pub fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        size: ScaledDimensions,
    ) -> UninitializedRenderBindings {
        let texture = device.create_texture(&Self::itercount_texture_desc(wgpu::Extent3d {
            width: size.aligned_width(64),
            height: size.height,
            depth_or_array_layers: 1,
        }));
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("FragmentParams"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
            ],
            label: None,
        });

        UninitializedRenderBindings(Self {
            bind_group,
            params_buffer,
            texture,
        })
    }

    pub fn write(&self, queue: &wgpu::Queue, params: FragmentParams) {
        let bytes: [u8; std::mem::size_of::<FragmentParams>()] = bytemuck::cast(params);
        queue.write_buffer(&self.params_buffer, 0, &bytes);
    }
}

pub struct UninitializedRenderBindings(RenderBindings);

impl UninitializedRenderBindings {
    pub fn write(self, queue: &wgpu::Queue, params: FragmentParams) -> RenderBindings {
        self.0.write(queue, params);
        self.0
    }
}
