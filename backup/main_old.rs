use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{DeviceEvent, ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowAttributes, WindowId},
};

const WORLD_X: i32 = 16;
const WORLD_Z: i32 = 16;
const FREE_CAMERA_SPEED: f32 = 10.0;
const PLAYER_MOVE_SPEED: f32 = 6.0;
const PLAYER_JUMP_SPEED: f32 = 8.5;
const GRAVITY: f32 = 26.0;
const PLAYER_RADIUS: f32 = 0.32;
const PLAYER_HEIGHT: f32 = 1.8;
const PLAYER_EYE_HEIGHT: f32 = 1.62;
const COLLISION_STEP: f32 = 0.05;
const LOOK_SENSITIVITY: f32 = 0.0025;
const MAX_PITCH: f32 = 1.54;
const OVERLAY_MARGIN: f32 = 16.0;
const OVERLAY_LINE_HEIGHT: f32 = 28.0;
const OVERLAY_GLYPH_SCALE: f32 = 3.0;
const FRAME_CAP_PRESETS: [Option<u32>; 5] = [None, Some(60), Some(120), Some(144), Some(240)];
const DEFAULT_FRAME_CAP_INDEX: usize = 3;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl Vertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

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
struct OverlayVertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl OverlayVertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<OverlayVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

#[derive(Clone, Copy)]
struct Camera {
    position: Vec3,
    yaw: f32,
    pitch: f32,
    aspect: f32,
    fov_y_radians: f32,
    z_near: f32,
    z_far: f32,
}

impl Camera {
    fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        )
        .normalize()
    }

    fn view_proj(&self) -> Mat4 {
        let view = Mat4::look_at_rh(self.position, self.position + self.forward(), Vec3::Y);
        let proj = Mat4::perspective_rh(self.fov_y_radians, self.aspect, self.z_near, self.z_far);

        // Convert OpenGL-style clip space to wgpu/WebGPU clip space.
        let opengl_to_wgpu = Mat4::from_cols_array(&[
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 0.5, 0.0,
            0.0, 0.0, 0.5, 1.0,
        ]);

        opengl_to_wgpu * proj * view
    }
}

struct InputState {
    pressed: HashSet<KeyCode>,
    mouse_captured: bool,
}

impl InputState {
    fn new() -> Self {
        Self {
            pressed: HashSet::new(),
            mouse_captured: false,
        }
    }

    fn key(&self, key: KeyCode) -> bool {
        self.pressed.contains(&key)
    }
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

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self { view }
    }
}

struct World {
    size_x: i32,
    size_y: i32,
    size_z: i32,
    blocks: Vec<bool>,
}

impl World {
    fn new(size_x: i32, size_y: i32, size_z: i32) -> Self {
        Self {
            size_x,
            size_y,
            size_z,
            blocks: vec![false; (size_x * size_y * size_z) as usize],
        }
    }

    fn flat(size_x: i32, size_z: i32) -> Self {
        let mut world = Self::new(size_x, 4, size_z);
        for x in 0..size_x {
            for z in 0..size_z {
                world.set_block(x, 0, z, true);
            }
        }
        world
    }

    fn index(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        if x < 0 || y < 0 || z < 0 || x >= self.size_x || y >= self.size_y || z >= self.size_z {
            return None;
        }

        Some(((y * self.size_z + z) * self.size_x + x) as usize)
    }

    fn set_block(&mut self, x: i32, y: i32, z: i32, solid: bool) {
        if let Some(index) = self.index(x, y, z) {
            self.blocks[index] = solid;
        }
    }

    fn is_solid(&self, x: i32, y: i32, z: i32) -> bool {
        self.index(x, y, z)
            .map(|index| self.blocks[index])
            .unwrap_or(false)
    }

    fn build_mesh(&self) -> Vec<Vertex> {
        let mut vertices = Vec::new();

        for y in 0..self.size_y {
            for x in 0..self.size_x {
                for z in 0..self.size_z {
                    if !self.is_solid(x, y, z) {
                        continue;
                    }

                    let base = Vec3::new(x as f32, y as f32, z as f32);
                    let top_color = if (x + z) % 2 == 0 {
                        [0.36, 0.78, 0.40]
                    } else {
                        [0.30, 0.70, 0.34]
                    };
                    let side_color = [0.24, 0.46, 0.22];
                    let bottom_color = [0.16, 0.22, 0.14];

                    if !self.is_solid(x, y + 1, z) {
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(0.0, 1.0, 0.0),
                                base + Vec3::new(1.0, 1.0, 0.0),
                                base + Vec3::new(1.0, 1.0, 1.0),
                                base + Vec3::new(0.0, 1.0, 1.0),
                            ],
                            top_color,
                        );
                    }
                    if !self.is_solid(x, y - 1, z) {
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(0.0, 0.0, 1.0),
                                base + Vec3::new(1.0, 0.0, 1.0),
                                base + Vec3::new(1.0, 0.0, 0.0),
                                base + Vec3::new(0.0, 0.0, 0.0),
                            ],
                            bottom_color,
                        );
                    }
                    if !self.is_solid(x - 1, y, z) {
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(0.0, 0.0, 0.0),
                                base + Vec3::new(0.0, 0.0, 1.0),
                                base + Vec3::new(0.0, 1.0, 1.0),
                                base + Vec3::new(0.0, 1.0, 0.0),
                            ],
                            side_color,
                        );
                    }
                    if !self.is_solid(x + 1, y, z) {
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(1.0, 0.0, 1.0),
                                base + Vec3::new(1.0, 0.0, 0.0),
                                base + Vec3::new(1.0, 1.0, 0.0),
                                base + Vec3::new(1.0, 1.0, 1.0),
                            ],
                            side_color,
                        );
                    }
                    if !self.is_solid(x, y, z - 1) {
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(1.0, 0.0, 0.0),
                                base + Vec3::new(0.0, 0.0, 0.0),
                                base + Vec3::new(0.0, 1.0, 0.0),
                                base + Vec3::new(1.0, 1.0, 0.0),
                            ],
                            side_color,
                        );
                    }
                    if !self.is_solid(x, y, z + 1) {
                        push_quad(
                            &mut vertices,
                            [
                                base + Vec3::new(0.0, 0.0, 1.0),
                                base + Vec3::new(1.0, 0.0, 1.0),
                                base + Vec3::new(1.0, 1.0, 1.0),
                                base + Vec3::new(0.0, 1.0, 1.0),
                            ],
                            side_color,
                        );
                    }
                }
            }
        }

        vertices
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CameraMode {
    Player,
    Free,
}

impl CameraMode {
    fn label(self) -> &'static str {
        match self {
            Self::Player => "player",
            Self::Free => "free",
        }
    }
}

struct PlayerController {
    position: Vec3,
    velocity: Vec3,
    yaw: f32,
    pitch: f32,
    on_ground: bool,
    aspect: f32,
}

impl PlayerController {
    fn new(aspect: f32) -> Self {
        Self {
            position: Vec3::new(8.0, 1.0, 8.0),
            velocity: Vec3::ZERO,
            yaw: -std::f32::consts::FRAC_PI_2,
            pitch: -0.35,
            on_ground: true,
            aspect,
        }
    }

    fn camera(&self) -> Camera {
        Camera {
            position: self.position + Vec3::Y * PLAYER_EYE_HEIGHT,
            yaw: self.yaw,
            pitch: self.pitch,
            aspect: self.aspect,
            fov_y_radians: 45.0_f32.to_radians(),
            z_near: 0.1,
            z_far: 500.0,
        }
    }

    fn forward_flat(&self) -> Vec3 {
        Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin()).normalize_or_zero()
    }
}

struct GpuState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
    camera: Camera,
    camera_uniform: CameraUniform,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    depth: DepthTexture,
    overlay_pipeline: wgpu::RenderPipeline,
    overlay_vertex_buffer: wgpu::Buffer,
    overlay_vertex_capacity: usize,
    screen_uniform: ScreenUniform,
    screen_uniform_buffer: wgpu::Buffer,
    screen_bind_group: wgpu::BindGroup,
}

enum RenderOutcome {
    Success,
    Reconfigure,
    SkipFrame,
    FatalSurfaceLoss,
}

impl GpuState {
    async fn new(window: Arc<Window>, world: &World, camera: Camera) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).expect("failed to create surface");

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

        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("surface is not supported by adapter");
        config.present_mode = wgpu::PresentMode::AutoNoVsync;
        surface.configure(&device, &config);

        let camera_uniform = CameraUniform {
            view_proj: camera.view_proj().to_cols_array_2d(),
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
        let screen_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("screen_uniform_buffer"),
            contents: bytemuck::bytes_of(&screen_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera_bind_group_layout"),
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

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera_bind_group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
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

        let vertices = world.build_mesh();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("world_vertex_buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
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
            vertex_buffer,
            vertex_count: vertices.len() as u32,
            camera,
            camera_uniform,
            camera_buffer,
            camera_bind_group,
            depth,
            overlay_pipeline,
            overlay_vertex_buffer,
            overlay_vertex_capacity,
            screen_uniform,
            screen_uniform_buffer,
            screen_bind_group,
        }
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }

        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.camera.aspect = self.config.width as f32 / self.config.height as f32;
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

    fn update_camera(&mut self) {
        self.camera_uniform.view_proj = self.camera.view_proj().to_cols_array_2d();
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[self.camera_uniform]),
        );
    }

    fn set_camera(&mut self, camera: Camera) {
        self.camera = camera;
        self.update_camera();
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

    fn estimated_gpu_memory_bytes(&self) -> u64 {
        let world_vertex_bytes = self.vertex_count as u64 * std::mem::size_of::<Vertex>() as u64;
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

    fn render(&mut self, overlay_vertices: &[OverlayVertex]) -> RenderOutcome {
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
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.04,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
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
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.draw(0..self.vertex_count, 0..1);
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
}

struct DebugOverlay {
    visible: bool,
    fps: f32,
    frame_count: u32,
    elapsed: f32,
}

impl DebugOverlay {
    fn new() -> Self {
        Self {
            visible: false,
            fps: 0.0,
            frame_count: 0,
            elapsed: 0.0,
        }
    }

    fn record_frame(&mut self, dt: f32) {
        self.frame_count += 1;
        self.elapsed += dt;

        if self.elapsed >= 0.25 {
            self.fps = self.frame_count as f32 / self.elapsed.max(f32::EPSILON);
            self.frame_count = 0;
            self.elapsed = 0.0;
        }
    }

    fn build_vertices(&self, memory_bytes: u64) -> Vec<OverlayVertex> {
        if !self.visible {
            return Vec::new();
        }

        let fps_line = format!("FPS {:.1}", self.fps);
        let memory_mb = memory_bytes as f64 / (1024.0 * 1024.0);
        let memory_line = format!("GPU MEM {:.2} MB", memory_mb);
        let lines = [fps_line, memory_line];

        let glyph_h = 7.0 * OVERLAY_GLYPH_SCALE;
        let advance = 6.0 * OVERLAY_GLYPH_SCALE;
        let panel_width = lines
            .iter()
            .map(|line| line.chars().count() as f32 * advance)
            .fold(0.0, f32::max)
            + 20.0;
        let panel_height = OVERLAY_MARGIN
            + lines.len() as f32 * OVERLAY_LINE_HEIGHT
            + glyph_h
            - OVERLAY_GLYPH_SCALE;

        let mut vertices = Vec::new();
        push_overlay_quad(
            &mut vertices,
            OVERLAY_MARGIN - 8.0,
            OVERLAY_MARGIN - 8.0,
            panel_width,
            panel_height,
            [0.02, 0.03, 0.05, 0.78],
        );
        push_overlay_quad(
            &mut vertices,
            OVERLAY_MARGIN - 4.0,
            OVERLAY_MARGIN - 4.0,
            panel_width,
            panel_height,
            [0.12, 0.16, 0.22, 0.35],
        );

        for (line_index, line) in lines.iter().enumerate() {
            let y = OVERLAY_MARGIN + line_index as f32 * OVERLAY_LINE_HEIGHT;
            push_text(
                &mut vertices,
                OVERLAY_MARGIN + 1.0,
                y + 1.0,
                line,
                OVERLAY_GLYPH_SCALE,
                [0.10, 0.16, 0.14, 0.85],
            );
            push_text(
                &mut vertices,
                OVERLAY_MARGIN,
                y,
                line,
                OVERLAY_GLYPH_SCALE,
                [0.82, 0.96, 0.88, 1.0],
            );
        }

        vertices
    }
}

fn push_quad(vertices: &mut Vec<Vertex>, corners: [Vec3; 4], color: [f32; 3]) {
    let [a, b, c, d] = corners;
    vertices.extend_from_slice(&[
        Vertex {
            position: a.to_array(),
            color,
        },
        Vertex {
            position: b.to_array(),
            color,
        },
        Vertex {
            position: c.to_array(),
            color,
        },
        Vertex {
            position: a.to_array(),
            color,
        },
        Vertex {
            position: c.to_array(),
            color,
        },
        Vertex {
            position: d.to_array(),
            color,
        },
    ]);
}

fn player_collides(world: &World, position: Vec3) -> bool {
    let min = Vec3::new(
        position.x - PLAYER_RADIUS,
        position.y,
        position.z - PLAYER_RADIUS,
    );
    let max = Vec3::new(
        position.x + PLAYER_RADIUS,
        position.y + PLAYER_HEIGHT,
        position.z + PLAYER_RADIUS,
    );

    let min_x = min.x.floor() as i32;
    let max_x = max.x.ceil() as i32 - 1;
    let min_y = min.y.floor() as i32;
    let max_y = max.y.ceil() as i32 - 1;
    let min_z = min.z.floor() as i32;
    let max_z = max.z.ceil() as i32 - 1;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            for z in min_z..=max_z {
                if world.is_solid(x, y, z) {
                    return true;
                }
            }
        }
    }

    false
}

fn move_axis(
    world: &World,
    mut position: Vec3,
    delta: Vec3,
    mut velocity_component: Option<&mut f32>,
    on_ground: &mut bool,
) -> Vec3 {
    let distance = delta.length();
    if distance <= f32::EPSILON {
        return position;
    }

    let steps = (distance / COLLISION_STEP).ceil() as i32;
    let step_delta = delta / steps as f32;

    for _ in 0..steps {
        let candidate = position + step_delta;
        if player_collides(world, candidate) {
            if step_delta.y < 0.0 {
                *on_ground = true;
            }
            if let Some(value) = velocity_component.as_deref_mut() {
                *value = 0.0;
            }
            break;
        }
        position = candidate;
    }

    position
}

fn push_overlay_quad(
    vertices: &mut Vec<OverlayVertex>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: [f32; 4],
) {
    let x2 = x + width;
    let y2 = y + height;
    let corners = [
        OverlayVertex {
            position: [x, y],
            color,
        },
        OverlayVertex {
            position: [x2, y],
            color,
        },
        OverlayVertex {
            position: [x2, y2],
            color,
        },
        OverlayVertex {
            position: [x, y],
            color,
        },
        OverlayVertex {
            position: [x2, y2],
            color,
        },
        OverlayVertex {
            position: [x, y2],
            color,
        },
    ];
    vertices.extend_from_slice(&corners);
}

fn push_text(
    vertices: &mut Vec<OverlayVertex>,
    x: f32,
    y: f32,
    text: &str,
    scale: f32,
    color: [f32; 4],
) {
    let mut cursor_x = x;
    for ch in text.chars() {
        let glyph = glyph_rows(ch);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5 {
                if (bits >> (4 - col)) & 1 == 1 {
                    push_overlay_quad(
                        vertices,
                        cursor_x + col as f32 * scale,
                        y + row as f32 * scale,
                        scale,
                        scale,
                        color,
                    );
                }
            }
        }
        cursor_x += 6.0 * scale;
    }
}

fn glyph_rows(ch: char) -> [u8; 7] {
    match ch {
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b11100],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110],
        'M' => [0b10001, 0b11011, 0b10101, 0b10001, 0b10001, 0b10001, 0b10001],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00110, 0b00110],
        ' ' => [0; 7],
        _ => [0; 7],
    }
}

struct App {
    window: Option<Arc<Window>>,
    window_id: Option<WindowId>,
    gpu: Option<GpuState>,
    world: World,
    input: InputState,
    player: PlayerController,
    free_camera: Camera,
    camera_mode: CameraMode,
    debug_overlay: DebugOverlay,
    frame_cap_index: usize,
    last_frame: Instant,
    next_frame_at: Instant,
}

impl App {
    fn new() -> Self {
        let aspect = 16.0 / 9.0;
        let player = PlayerController::new(aspect);
        Self {
            window: None,
            window_id: None,
            gpu: None,
            world: World::flat(WORLD_X, WORLD_Z),
            input: InputState::new(),
            free_camera: player.camera(),
            player,
            camera_mode: CameraMode::Player,
            debug_overlay: DebugOverlay::new(),
            frame_cap_index: DEFAULT_FRAME_CAP_INDEX,
            last_frame: Instant::now(),
            next_frame_at: Instant::now(),
        }
    }

    fn current_frame_cap(&self) -> Option<u32> {
        FRAME_CAP_PRESETS[self.frame_cap_index]
    }

    fn frame_cap_label(&self) -> String {
        match self.current_frame_cap() {
            Some(fps) => format!("{fps} FPS cap"),
            None => "uncapped".to_string(),
        }
    }

    fn active_camera(&self) -> Camera {
        match self.camera_mode {
            CameraMode::Player => self.player.camera(),
            CameraMode::Free => self.free_camera,
        }
    }

    fn sync_active_camera(&mut self) {
        let camera = self.active_camera();
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.set_camera(camera);
        }
    }

    fn refresh_window_title(&self) {
        if let Some(window) = &self.window {
            window.set_title(&format!(
                "Voxel Starter [{} | {}]",
                self.frame_cap_label(),
                self.camera_mode.label()
            ));
        }
    }

    fn cycle_frame_cap(&mut self) {
        self.frame_cap_index = (self.frame_cap_index + 1) % FRAME_CAP_PRESETS.len();
        self.next_frame_at = Instant::now();
        self.refresh_window_title();
    }

    fn toggle_camera_mode(&mut self) {
        self.camera_mode = match self.camera_mode {
            CameraMode::Player => {
                self.free_camera = self.player.camera();
                CameraMode::Free
            }
            CameraMode::Free => CameraMode::Player,
        };
        self.sync_active_camera();
        self.refresh_window_title();
    }

    fn capture_mouse(&mut self) {
        if let Some(window) = &self.window {
            let _ = window.set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
            window.set_cursor_visible(false);
            self.input.mouse_captured = true;
        }
    }

    fn release_mouse(&mut self) {
        if let Some(window) = &self.window {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
            self.input.mouse_captured = false;
        }
    }

    fn update(&mut self, dt: f32) {
        match self.camera_mode {
            CameraMode::Free => self.update_free_camera(dt),
            CameraMode::Player => self.update_player(dt),
        }
        self.sync_active_camera();
    }

    fn update_free_camera(&mut self, dt: f32) {
        let forward = self.free_camera.forward();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let mut move_dir = Vec3::ZERO;

        if self.input.key(KeyCode::KeyW) {
            move_dir += forward;
        }
        if self.input.key(KeyCode::KeyS) {
            move_dir -= forward;
        }
        if self.input.key(KeyCode::KeyA) {
            move_dir -= right;
        }
        if self.input.key(KeyCode::KeyD) {
            move_dir += right;
        }
        if self.input.key(KeyCode::Space) {
            move_dir += Vec3::Y;
        }
        if self.input.key(KeyCode::ShiftLeft) || self.input.key(KeyCode::ShiftRight) {
            move_dir -= Vec3::Y;
        }

        if move_dir.length_squared() > 0.0 {
            self.free_camera.position += move_dir.normalize() * FREE_CAMERA_SPEED * dt;
        }
    }

    fn update_player(&mut self, dt: f32) {
        let mut move_dir = Vec3::ZERO;
        let forward_flat = self.player.forward_flat();
        let right_flat = forward_flat.cross(Vec3::Y).normalize_or_zero();

        if self.input.key(KeyCode::KeyW) {
            move_dir += forward_flat;
        }
        if self.input.key(KeyCode::KeyS) {
            move_dir -= forward_flat;
        }
        if self.input.key(KeyCode::KeyA) {
            move_dir -= right_flat;
        }
        if self.input.key(KeyCode::KeyD) {
            move_dir += right_flat;
        }

        let horizontal_velocity = if move_dir.length_squared() > 0.0 {
            move_dir.normalize() * PLAYER_MOVE_SPEED
        } else {
            Vec3::ZERO
        };

        if self.input.key(KeyCode::Space) && self.player.on_ground {
            self.player.velocity.y = PLAYER_JUMP_SPEED;
            self.player.on_ground = false;
        }

        self.player.velocity.x = horizontal_velocity.x;
        self.player.velocity.z = horizontal_velocity.z;
        self.player.velocity.y -= GRAVITY * dt;

        self.player.position = move_axis(
            &self.world,
            self.player.position,
            Vec3::new(self.player.velocity.x * dt, 0.0, 0.0),
            None,
            &mut self.player.on_ground,
        );
        self.player.position = move_axis(
            &self.world,
            self.player.position,
            Vec3::new(0.0, 0.0, self.player.velocity.z * dt),
            None,
            &mut self.player.on_ground,
        );
        self.player.on_ground = false;
        self.player.position = move_axis(
            &self.world,
            self.player.position,
            Vec3::new(0.0, self.player.velocity.y * dt, 0.0),
            Some(&mut self.player.velocity.y),
            &mut self.player.on_ground,
        );
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = WindowAttributes::default()
            .with_title("Voxel Starter")
            .with_inner_size(PhysicalSize::new(1280, 720));

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("failed to create window"),
        );

        let size = window.inner_size();
        let aspect = size.width.max(1) as f32 / size.height.max(1) as f32;
        self.player.aspect = aspect;
        self.free_camera.aspect = aspect;
        let window_id = window.id();
        let gpu = pollster::block_on(GpuState::new(
            window.clone(),
            &self.world,
            self.active_camera(),
        ));

        self.window = Some(window);
        self.window_id = Some(window_id);
        self.gpu = Some(gpu);
        self.last_frame = Instant::now();
        self.next_frame_at = self.last_frame;
        self.refresh_window_title();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if Some(window_id) != self.window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.resize(size);
                }
                let aspect = size.width.max(1) as f32 / size.height.max(1) as f32;
                self.player.aspect = aspect;
                self.free_camera.aspect = aspect;
                self.sync_active_camera();
            }
            WindowEvent::RedrawRequested => {
                if let Some(gpu) = self.gpu.as_mut() {
                    let overlay_vertices = self
                        .debug_overlay
                        .build_vertices(gpu.estimated_gpu_memory_bytes());
                    match gpu.render(&overlay_vertices) {
                        RenderOutcome::Success | RenderOutcome::SkipFrame => {}
                        RenderOutcome::Reconfigure => {
                            if let Some(window) = &self.window {
                                gpu.resize(window.inner_size());
                            }
                        }
                        RenderOutcome::FatalSurfaceLoss => {
                            event_loop.exit();
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => {
                            self.input.pressed.insert(code);

                            if code == KeyCode::F3 && !event.repeat {
                                self.debug_overlay.visible = !self.debug_overlay.visible;
                            }
                            if code == KeyCode::F4 && !event.repeat {
                                self.cycle_frame_cap();
                            }
                            if code == KeyCode::F5 && !event.repeat {
                                self.toggle_camera_mode();
                            }
                            if code == KeyCode::Escape {
                                self.release_mouse();
                            }
                        }
                        ElementState::Released => {
                            self.input.pressed.remove(&code);
                        }
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                self.capture_mouse();
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if !self.input.mouse_captured {
            return;
        }

        if let DeviceEvent::MouseMotion { delta } = event {
            match self.camera_mode {
                CameraMode::Player => {
                    self.player.yaw += delta.0 as f32 * LOOK_SENSITIVITY;
                    self.player.pitch -= delta.1 as f32 * LOOK_SENSITIVITY;
                    self.player.pitch = self.player.pitch.clamp(-MAX_PITCH, MAX_PITCH);
                }
                CameraMode::Free => {
                    self.free_camera.yaw += delta.0 as f32 * LOOK_SENSITIVITY;
                    self.free_camera.pitch -= delta.1 as f32 * LOOK_SENSITIVITY;
                    self.free_camera.pitch = self.free_camera.pitch.clamp(-MAX_PITCH, MAX_PITCH);
                }
            }
            self.sync_active_camera();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if let Some(target_fps) = self.current_frame_cap() {
            let frame_interval = Duration::from_secs_f64(1.0 / target_fps as f64);
            if now < self.next_frame_at {
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_at));
                return;
            }
            self.next_frame_at = now + frame_interval;
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_at));
        } else {
            self.next_frame_at = now;
            event_loop.set_control_flow(ControlFlow::Poll);
        }

        let dt = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;

        self.debug_overlay.record_frame(dt);
        self.update(dt);

        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("event loop error");
}
