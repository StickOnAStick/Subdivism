use std::{collections::HashMap, sync::Arc};

use bytemuck::{Pod, Zeroable};
use glam::{Vec3, Vec4};
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

use crate::{
    camera::Camera,
    debug_overlay::OverlayVertex,
    game::world::{CHUNK_SIZE, WORLD_HEIGHT},
    mesh::Vertex,
};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ScreenUniform {
    screen_size: [f32; 2],
    _padding: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LightingUniform {
    sun_direction: [f32; 4],
    moon_direction: [f32; 4],
    sky_top: [f32; 4],
    sky_bottom: [f32; 4],
    camera_position: [f32; 4],
    params: [f32; 4],
}

#[derive(Clone, Copy)]
pub struct CelestialState {
    pub sun_direction: Vec3,
    pub moon_direction: Vec3,
    pub sky_top: [f32; 3],
    pub sky_bottom: [f32; 3],
    pub sun_intensity: f32,
    pub moon_intensity: f32,
    pub ambient: f32,
}

struct DepthTexture {
    view: wgpu::TextureView,
}

impl DepthTexture {
    const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    fn create(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth_texture"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        Self {
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
        }
    }
}

struct ChunkMeshGpu {
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    vertex_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkRenderKey {
    pub origin_chunk: (i64, i64),
    pub lod_level: u8,
}

pub enum RenderOutcome {
    Success,
    Reconfigure,
    SkipFrame,
    FatalSurfaceLoss,
}

pub struct GpuState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    world_chunks: HashMap<ChunkRenderKey, ChunkMeshGpu>,
    camera: Camera,
    camera_uniform: CameraUniform,
    camera_buffer: wgpu::Buffer,
    lighting_uniform: LightingUniform,
    lighting_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    clear_color: wgpu::Color,
    depth: DepthTexture,
    overlay_pipeline: wgpu::RenderPipeline,
    overlay_vertex_buffer: wgpu::Buffer,
    overlay_vertex_capacity: usize,
    screen_uniform: ScreenUniform,
    screen_uniform_buffer: wgpu::Buffer,
    screen_bind_group: wgpu::BindGroup,
}

impl GpuState {
    pub async fn new(window: Arc<Window>, camera: Camera) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window)
            .expect("failed to create surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("no suitable GPU adapters found");

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::default(),
            })
            .await
            .expect("failed to create device");

        let caps = surface.get_capabilities(&adapter);
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("surface is not supported by adapter");
        config.present_mode = if caps.present_modes.contains(&wgpu::PresentMode::Immediate) {
            wgpu::PresentMode::Immediate
        } else {
            wgpu::PresentMode::AutoNoVsync
        };
        surface.configure(&device, &config);

        let camera_uniform = CameraUniform {
            view_proj: camera.view_proj().to_cols_array_2d(),
        };
        let celestial = celestial_state_for_time(0.0);
        let lighting_uniform = LightingUniform {
            sun_direction: [
                celestial.sun_direction.x,
                celestial.sun_direction.y,
                celestial.sun_direction.z,
                0.0,
            ],
            moon_direction: [
                celestial.moon_direction.x,
                celestial.moon_direction.y,
                celestial.moon_direction.z,
                0.0,
            ],
            sky_top: [
                celestial.sky_top[0],
                celestial.sky_top[1],
                celestial.sky_top[2],
                1.0,
            ],
            sky_bottom: [
                celestial.sky_bottom[0],
                celestial.sky_bottom[1],
                celestial.sky_bottom[2],
                1.0,
            ],
            camera_position: [camera.position.x, camera.position.y, camera.position.z, 0.0],
            params: [
                celestial.sun_intensity,
                celestial.moon_intensity,
                celestial.ambient,
                0.0,
            ],
        };
        let screen_uniform = ScreenUniform {
            screen_size: [config.width as f32, config.height as f32],
            _padding: [0.0; 2],
        };

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera_buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let lighting_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("lighting_buffer"),
            contents: bytemuck::bytes_of(&lighting_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let screen_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("screen_uniform_buffer"),
            contents: bytemuck::bytes_of(&screen_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera_bind_group_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera_bind_group"),
            layout: &camera_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: lighting_buffer.as_entire_binding(),
                },
            ],
        });

        let screen_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("screen_bind_group_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let screen_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("screen_bind_group"),
            layout: &screen_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen_uniform_buffer.as_entire_binding(),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("overlay_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("overlay.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline_layout"),
            bind_group_layouts: &[Some(&camera_bind_group_layout)],
            immediate_size: 0,
        });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DepthTexture::FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let overlay_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("overlay_pipeline_layout"),
                bind_group_layouts: &[Some(&screen_bind_group_layout)],
                immediate_size: 0,
            });
        let overlay_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("overlay_pipeline"),
            layout: Some(&overlay_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &overlay_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[OverlayVertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &overlay_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let overlay_vertex_capacity = 1024;
        let overlay_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("overlay_vertex_buffer"),
            size: (overlay_vertex_capacity * std::mem::size_of::<OverlayVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let depth = DepthTexture::create(&device, &config);

        Self {
            surface,
            device,
            queue,
            config,
            render_pipeline,
            world_chunks: HashMap::new(),
            camera,
            camera_uniform,
            camera_buffer,
            lighting_uniform,
            lighting_buffer,
            camera_bind_group,
            clear_color: wgpu::Color {
                r: celestial.sky_bottom[0] as f64,
                g: celestial.sky_bottom[1] as f64,
                b: celestial.sky_bottom[2] as f64,
                a: 1.0,
            },
            depth,
            overlay_pipeline,
            overlay_vertex_buffer,
            overlay_vertex_capacity,
            screen_uniform,
            screen_uniform_buffer,
            screen_bind_group,
        }
    }

    pub fn upsert_chunk_mesh(&mut self, key: ChunkRenderKey, vertices: &[Vertex]) {
        let needed = vertices.len().max(1);
        let entry = self
            .world_chunks
            .entry(key)
            .or_insert_with(|| ChunkMeshGpu {
                vertex_buffer: self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("world_chunk_vertex_buffer"),
                    size: (needed.next_power_of_two() * std::mem::size_of::<Vertex>()) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                vertex_capacity: needed.next_power_of_two(),
                vertex_count: 0,
            });

        if needed > entry.vertex_capacity {
            entry.vertex_capacity = needed.next_power_of_two();
            entry.vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("world_chunk_vertex_buffer"),
                size: (entry.vertex_capacity * std::mem::size_of::<Vertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        entry.vertex_count = vertices.len() as u32;
        if !vertices.is_empty() {
            self.queue
                .write_buffer(&entry.vertex_buffer, 0, bytemuck::cast_slice(vertices));
        }
    }

    pub fn remove_chunk_mesh(&mut self, key: ChunkRenderKey) {
        self.world_chunks.remove(&key);
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }

        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.camera.lens.aspect = self.config.width as f32 / self.config.height as f32;
        self.surface.configure(&self.device, &self.config);
        self.depth = DepthTexture::create(&self.device, &self.config);
        self.screen_uniform.screen_size = [self.config.width as f32, self.config.height as f32];
        self.queue.write_buffer(
            &self.screen_uniform_buffer,
            0,
            bytemuck::bytes_of(&self.screen_uniform),
        );
        self.update_camera();
    }

    pub fn set_camera(&mut self, camera: Camera) {
        self.camera = camera;
        self.update_camera();
    }

    pub fn set_environment(
        &mut self,
        time_seconds: f32,
        camera_position: Vec3,
        terrain_seed: i64,
    ) -> CelestialState {
        let celestial = celestial_state_for_time(time_seconds);
        self.lighting_uniform.sun_direction = [
            celestial.sun_direction.x,
            celestial.sun_direction.y,
            celestial.sun_direction.z,
            0.0,
        ];
        self.lighting_uniform.moon_direction = [
            celestial.moon_direction.x,
            celestial.moon_direction.y,
            celestial.moon_direction.z,
            0.0,
        ];
        self.lighting_uniform.sky_top = [
            celestial.sky_top[0],
            celestial.sky_top[1],
            celestial.sky_top[2],
            1.0,
        ];
        self.lighting_uniform.sky_bottom = [
            celestial.sky_bottom[0],
            celestial.sky_bottom[1],
            celestial.sky_bottom[2],
            1.0,
        ];
        self.lighting_uniform.camera_position =
            [camera_position.x, camera_position.y, camera_position.z, 0.0];
        self.lighting_uniform.params = [
            celestial.sun_intensity,
            celestial.moon_intensity,
            celestial.ambient,
            terrain_seed as f32,
        ];
        self.clear_color = wgpu::Color {
            r: celestial.sky_bottom[0] as f64,
            g: celestial.sky_bottom[1] as f64,
            b: celestial.sky_bottom[2] as f64,
            a: 1.0,
        };
        self.queue.write_buffer(
            &self.lighting_buffer,
            0,
            bytemuck::bytes_of(&self.lighting_uniform),
        );
        celestial
    }

    pub fn estimated_gpu_memory_bytes(&self) -> u64 {
        let world_vertex_bytes = self
            .world_chunks
            .values()
            .map(|chunk| chunk.vertex_capacity as u64 * std::mem::size_of::<Vertex>() as u64)
            .sum::<u64>();
        let camera_uniform_bytes = std::mem::size_of::<CameraUniform>() as u64;
        let screen_uniform_bytes = std::mem::size_of::<ScreenUniform>() as u64;
        let depth_texture_bytes = self.config.width as u64 * self.config.height as u64 * 4;
        let overlay_vertex_bytes =
            self.overlay_vertex_capacity as u64 * std::mem::size_of::<OverlayVertex>() as u64;

        world_vertex_bytes
            + camera_uniform_bytes
            + screen_uniform_bytes
            + depth_texture_bytes
            + overlay_vertex_bytes
    }

    pub fn render(&mut self, overlay_vertices: &[OverlayVertex]) -> RenderOutcome {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated => return RenderOutcome::Reconfigure,
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return RenderOutcome::SkipFrame,
            wgpu::CurrentSurfaceTexture::Lost => return RenderOutcome::FatalSurfaceLoss,
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        if !overlay_vertices.is_empty() {
            self.ensure_overlay_capacity(overlay_vertices.len());
            self.queue.write_buffer(
                &self.overlay_vertex_buffer,
                0,
                bytemuck::cast_slice(overlay_vertices),
            );
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render_encoder"),
            });

        {
            let cull_context = ChunkCullContext::new(self.camera);
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            for (key, chunk) in &self.world_chunks {
                if chunk.vertex_count == 0 {
                    continue;
                }
                if !cull_context.chunk_in_view(*key) {
                    continue;
                }
                render_pass.set_vertex_buffer(0, chunk.vertex_buffer.slice(..));
                render_pass.draw(0..chunk.vertex_count, 0..1);
            }
        }

        if !overlay_vertices.is_empty() {
            let mut overlay_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("overlay_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            overlay_pass.set_pipeline(&self.overlay_pipeline);
            overlay_pass.set_bind_group(0, &self.screen_bind_group, &[]);
            overlay_pass.set_vertex_buffer(0, self.overlay_vertex_buffer.slice(..));
            overlay_pass.draw(0..overlay_vertices.len() as u32, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        RenderOutcome::Success
    }

    fn update_camera(&mut self) {
        self.camera_uniform.view_proj = self.camera.view_proj().to_cols_array_2d();
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[self.camera_uniform]),
        );
    }

    fn ensure_overlay_capacity(&mut self, vertex_count: usize) {
        if vertex_count <= self.overlay_vertex_capacity {
            return;
        }

        self.overlay_vertex_capacity = vertex_count.next_power_of_two();
        self.overlay_vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("overlay_vertex_buffer"),
            size: (self.overlay_vertex_capacity * std::mem::size_of::<OverlayVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
    }
}

pub fn celestial_state_for_time(time_seconds: f32) -> CelestialState {
    let day_length_seconds = 180.0;
    let phase = (time_seconds / day_length_seconds).fract() * std::f32::consts::TAU;
    let sun_direction = Vec3::new(phase.cos() * 0.55, phase.sin(), phase.sin() * 0.55).normalize();
    let moon_direction = -sun_direction;
    let sun_elevation = sun_direction.y.clamp(-1.0, 1.0);
    let sun_up = sun_elevation.max(0.0);
    let moon_up = (-sun_elevation).max(0.0);
    let horizon_blend = (1.0 - sun_elevation.abs()).powf(2.2);

    let day_top = Vec3::new(0.33, 0.63, 0.95);
    let day_bottom = Vec3::new(0.62, 0.82, 0.99);
    let night_top = Vec3::new(0.03, 0.05, 0.11);
    let night_bottom = Vec3::new(0.06, 0.09, 0.17);
    let sunrise = Vec3::new(1.00, 0.50, 0.30);
    let sunset = Vec3::new(1.00, 0.66, 0.42);
    let sherbet = (sunrise + sunset) * 0.5;

    let daylight = sun_up.powf(0.42);
    let mut sky_top = night_top.lerp(day_top, daylight);
    let mut sky_bottom = night_bottom.lerp(day_bottom, daylight);
    sky_top = sky_top.lerp(sherbet, horizon_blend * 0.35);
    sky_bottom = sky_bottom.lerp(sherbet, horizon_blend * 0.55);

    let sun_intensity = (sun_up * 1.05).powf(0.75);
    let moon_intensity = (moon_up * 0.38).powf(0.8);
    let ambient = 0.06 + sun_intensity * 0.22 + moon_intensity * 0.16;

    CelestialState {
        sun_direction,
        moon_direction,
        sky_top: sky_top.to_array(),
        sky_bottom: sky_bottom.to_array(),
        sun_intensity,
        moon_intensity,
        ambient,
    }
}

struct ChunkCullContext {
    frustum_planes: [Vec4; 5],
    camera_position: Vec3,
}

impl ChunkCullContext {
    fn new(camera: Camera) -> Self {
        let m = camera.view_proj().to_cols_array_2d();
        let r0 = Vec4::new(m[0][0], m[1][0], m[2][0], m[3][0]);
        let r1 = Vec4::new(m[0][1], m[1][1], m[2][1], m[3][1]);
        let r2 = Vec4::new(m[0][2], m[1][2], m[2][2], m[3][2]);
        let r3 = Vec4::new(m[0][3], m[1][3], m[2][3], m[3][3]);

        let planes = [
            normalize_plane(r3 + r0),
            normalize_plane(r3 - r0),
            normalize_plane(r3 + r1),
            normalize_plane(r3 - r1),
            normalize_plane(r3 - r2),
        ];

        Self {
            frustum_planes: planes,
            camera_position: camera.position,
        }
    }

    fn chunk_in_view(&self, key: ChunkRenderKey) -> bool {
        let span = 1_i64 << key.lod_level;
        let world_span = CHUNK_SIZE * span;
        let min = Vec3::new(
            (key.origin_chunk.0 * CHUNK_SIZE) as f32,
            0.0,
            (key.origin_chunk.1 * CHUNK_SIZE) as f32,
        );
        let max = Vec3::new(
            min.x + world_span as f32,
            WORLD_HEIGHT as f32,
            min.z + world_span as f32,
        );

        // Keep nearby chunks resident even if frustum math is on the edge for a frame.
        let closest = Vec3::new(
            self.camera_position.x.clamp(min.x, max.x),
            self.camera_position.y.clamp(min.y, max.y),
            self.camera_position.z.clamp(min.z, max.z),
        );
        let close_radius = (world_span as f32 * 1.5).max(CHUNK_SIZE as f32 * 3.0);
        if self.camera_position.distance_squared(closest) <= close_radius * close_radius {
            return true;
        }

        let cull_slack = world_span as f32 * 0.35 + 2.0;
        for plane in self.frustum_planes {
            let positive = Vec3::new(
                if plane.x >= 0.0 { max.x } else { min.x },
                if plane.y >= 0.0 { max.y } else { min.y },
                if plane.z >= 0.0 { max.z } else { min.z },
            );
            let distance =
                plane.x * positive.x + plane.y * positive.y + plane.z * positive.z + plane.w;
            if distance < -cull_slack {
                return false;
            }
        }
        true
    }
}

fn normalize_plane(plane: Vec4) -> Vec4 {
    let normal = Vec3::new(plane.x, plane.y, plane.z);
    let length = normal.length();
    if length <= f32::EPSILON {
        return plane;
    }
    plane / length
}
