use std::{
    collections::HashSet,
    sync::Arc,
    time::Instant,
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
const MOVE_SPEED: f32 = 8.0;
const SPRINT_MULTIPLIER: f32 = 2.0;
const LOOK_SENSITIVITY: f32 = 0.0025;
const MAX_PITCH: f32 = 1.54;

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
}

impl GpuState {
    async fn new(window: Arc<Window>) -> Self {
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
        config.present_mode = wgpu::PresentMode::Fifo;
        surface.configure(&device, &config);

        let camera = Camera {
            position: Vec3::new(8.0, 10.0, 20.0),
            yaw: -std::f32::consts::FRAC_PI_2,
            pitch: -0.35,
            aspect: config.width as f32 / config.height as f32,
            fov_y_radians: 45.0_f32.to_radians(),
            z_near: 0.1,
            z_far: 500.0,
        };

        let camera_uniform = CameraUniform {
            view_proj: camera.view_proj().to_cols_array_2d(),
        };

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera_buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
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

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
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
                topology: wgpu::PrimitiveTopology::LineList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
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

        let vertices = build_flat_world_wireframe(WORLD_X, WORLD_Z);
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("world_vertex_buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
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

    fn render(&mut self) -> Result<(), wgpu::CurrentSurfaceTexture> {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            other => return Err(other),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

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

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }
}

fn push_line(vertices: &mut Vec<Vertex>, a: Vec3, b: Vec3, color: [f32; 3]) {
    vertices.push(Vertex {
        position: a.to_array(),
        color,
    });
    vertices.push(Vertex {
        position: b.to_array(),
        color,
    });
}

fn add_wire_cube(vertices: &mut Vec<Vertex>, base: Vec3, color: [f32; 3]) {
    let p000 = base + Vec3::new(0.0, 0.0, 0.0);
    let p100 = base + Vec3::new(1.0, 0.0, 0.0);
    let p010 = base + Vec3::new(0.0, 1.0, 0.0);
    let p110 = base + Vec3::new(1.0, 1.0, 0.0);
    let p001 = base + Vec3::new(0.0, 0.0, 1.0);
    let p101 = base + Vec3::new(1.0, 0.0, 1.0);
    let p011 = base + Vec3::new(0.0, 1.0, 1.0);
    let p111 = base + Vec3::new(1.0, 1.0, 1.0);

    // Bottom rectangle
    push_line(vertices, p000, p100, color);
    push_line(vertices, p100, p101, color);
    push_line(vertices, p101, p001, color);
    push_line(vertices, p001, p000, color);

    // Top rectangle
    push_line(vertices, p010, p110, color);
    push_line(vertices, p110, p111, color);
    push_line(vertices, p111, p011, color);
    push_line(vertices, p011, p010, color);

    // Vertical edges
    push_line(vertices, p000, p010, color);
    push_line(vertices, p100, p110, color);
    push_line(vertices, p101, p111, color);
    push_line(vertices, p001, p011, color);
}

fn build_flat_world_wireframe(size_x: i32, size_z: i32) -> Vec<Vertex> {
    let mut vertices = Vec::new();

    for x in 0..size_x {
        for z in 0..size_z {
            let color = if (x + z) % 2 == 0 {
                [0.35, 0.85, 0.35]
            } else {
                [0.25, 0.70, 0.25]
            };
            add_wire_cube(&mut vertices, Vec3::new(x as f32, 0.0, z as f32), color);
        }
    }

    vertices
}

struct App {
    window: Option<Arc<Window>>,
    window_id: Option<WindowId>,
    gpu: Option<GpuState>,
    input: InputState,
    last_frame: Instant,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            window_id: None,
            gpu: None,
            input: InputState::new(),
            last_frame: Instant::now(),
        }
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
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };

        let mut move_dir = Vec3::ZERO;
        let forward_flat =
            Vec3::new(gpu.camera.forward().x, 0.0, gpu.camera.forward().z).normalize_or_zero();
        let right_flat = forward_flat.cross(Vec3::Y).normalize_or_zero();
        let speed = if self.input.key(KeyCode::ShiftLeft) || self.input.key(KeyCode::ShiftRight) {
            MOVE_SPEED * SPRINT_MULTIPLIER
        } else {
            MOVE_SPEED
        };

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
        if self.input.key(KeyCode::Space) {
            move_dir += Vec3::Y;
        }
        if self.input.key(KeyCode::ControlLeft) || self.input.key(KeyCode::ControlRight) {
            move_dir -= Vec3::Y;
        }

        if move_dir.length_squared() > 0.0 {
            gpu.camera.position += move_dir.normalize() * speed * dt;
            gpu.update_camera();
        }
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

        let window_id = window.id();
        let gpu = pollster::block_on(GpuState::new(window.clone()));

        self.window = Some(window);
        self.window_id = Some(window_id);
        self.gpu = Some(gpu);
        self.last_frame = Instant::now();
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
            }
            WindowEvent::RedrawRequested => {
                if let Some(gpu) = self.gpu.as_mut() {
                    match gpu.render() {
                        Ok(_) => {}
                        Err(
                            wgpu::CurrentSurfaceTexture::Lost
                            | wgpu::CurrentSurfaceTexture::Outdated
                            | wgpu::CurrentSurfaceTexture::Suboptimal(_),
                        ) => {
                            if let Some(window) = &self.window {
                                gpu.resize(window.inner_size());
                            }
                        }
                        Err(wgpu::CurrentSurfaceTexture::Timeout)
                        | Err(wgpu::CurrentSurfaceTexture::Occluded) => {}
                        Err(wgpu::CurrentSurfaceTexture::Validation) => {}
                        Err(_) => {}
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => {
                            self.input.pressed.insert(code);

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

        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };

        if let DeviceEvent::MouseMotion { delta } = event {
            gpu.camera.yaw += delta.0 as f32 * LOOK_SENSITIVITY;
            gpu.camera.pitch -= delta.1 as f32 * LOOK_SENSITIVITY;
            gpu.camera.pitch = gpu.camera.pitch.clamp(-MAX_PITCH, MAX_PITCH);
            gpu.update_camera();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Poll);

        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;

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
