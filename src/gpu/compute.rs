use crate::{float::WideFloat, primitives::PrecisePoint, Dimensions, WORD_COUNT};

#[derive(Debug, Clone)]
pub struct ComputeParams<'p, 's> {
    iteration_limit: u32,
    reset: bool,
    size: Dimensions,
    top_left: &'p PrecisePoint,
    step: &'s WideFloat<WORD_COUNT>,
}

pub struct ComputeBindings {
    pub(super) bind_group: wgpu::BindGroup,
    pub(super) params_buffer: wgpu::Buffer,
    pub(super) _intermediate_buffer: wgpu::Buffer,
    pub(super) result_buffer: wgpu::Buffer,
}

impl ComputeBindings {
    pub const fn bind_group_layout_desc() -> wgpu::BindGroupLayoutDescriptor<'static> {
        wgpu::BindGroupLayoutDescriptor {
            label: Some("Compute BindGroupLayout"),
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
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        }
    }

    pub fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        size: Dimensions,
    ) -> UninitializedComputeBindings {
        // Buffer to pass input parameters to the GPU
        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Compute Params"),
            size: ComputeParams::size_hint() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Buffer with the cache for iterative computation
        let intermediate_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Compute Intermediate"),
            size: (2 * WORD_COUNT as u32 * 4 * size.aligned_width(64) * size.height) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        // Buffer with result produced by the GPU
        let result_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Compute Result"),
            size: (4 * size.aligned_width(64) * size.height) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
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
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: intermediate_buffer.as_entire_binding(),
                },
            ],
        });

        UninitializedComputeBindings(Self {
            params_buffer,
            _intermediate_buffer: intermediate_buffer,
            result_buffer,
            bind_group,
        })
    }

    pub fn write(&self, queue: &wgpu::Queue, params: &ComputeParams) {
        queue.write_buffer(&self.params_buffer, 0, &params.encode());
    }

    pub fn write_iterate(&self, queue: &wgpu::Queue, new_iteration_limit: u32) {
        // Unset reset flag and write new iteration limit
        let mut buffer = [0; 8];
        buffer[0..4].copy_from_slice(&bytemuck::cast::<_, [u8; 4]>(new_iteration_limit));
        buffer[4..8].copy_from_slice(&[0, 0, 0, 0]);
        queue.write_buffer(&self.params_buffer, 0, &buffer);
    }
}

pub struct UninitializedComputeBindings(ComputeBindings);

impl UninitializedComputeBindings {
    pub fn write(self, queue: &wgpu::Queue, params: &ComputeParams) -> ComputeBindings {
        self.0.write(queue, params);
        self.0
    }
}

impl<'p, 's> ComputeParams<'p, 's> {
    pub fn new(
        size: Dimensions,
        top_left: &'p PrecisePoint,
        step: &'s WideFloat<WORD_COUNT>,
        iteration_limit: u32,
    ) -> Self {
        Self {
            size,
            top_left,
            step,
            iteration_limit,
            reset: true,
        }
    }

    fn encode(&self) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(Self::size_hint() as usize);
        buffer.extend_from_slice(&bytemuck::cast::<_, [u8; 4]>(self.iteration_limit));
        buffer.extend_from_slice(&bytemuck::cast::<_, [u8; 4]>(self.reset as u32));
        buffer.extend_from_slice(&bytemuck::cast::<_, [u8; 4]>(self.size.aligned_width(64)));
        buffer.extend_from_slice(&bytemuck::cast::<_, [u8; 4]>(self.size.height));
        buffer.extend_from_slice(bytemuck::cast_slice(&self.top_left.x.0));
        buffer.extend_from_slice(bytemuck::cast_slice(&self.top_left.y.0));
        buffer.extend_from_slice(bytemuck::cast_slice(&self.step.0));
        buffer
    }

    const fn size_hint() -> u32 {
        WORD_COUNT as u32 * 12 + 16
    }
}
