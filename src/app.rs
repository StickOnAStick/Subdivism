use std::{
    collections::{HashMap, HashSet},
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use glam::Vec3;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{DeviceEvent, ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowAttributes, WindowId},
};

use crate::{
    camera::{Camera, CameraLens},
    debug_overlay::{DebugOverlay, OverlayVertex},
    game::{
        actor::{ActorRoster, PLAYER_EYE_HEIGHT},
        inventory::Inventory,
        physics::{self, MovementInput, PhysicsConfig},
        world::{Block, World},
    },
    mesh::Vertex,
    render::{celestial_state_for_time, GpuState, RenderOutcome},
};

const FREE_CAMERA_SPEED: f32 = 10.0;
const LOOK_SENSITIVITY: f32 = 0.0025;
const MAX_PITCH: f32 = 1.54;
const FRAME_CAP_PRESETS: [Option<u32>; 5] = [None, Some(60), Some(120), Some(144), Some(240)];
const DEFAULT_FRAME_CAP_INDEX: usize = 3;
const DEFAULT_RENDER_DISTANCE_CHUNKS: u32 = 10;
const MIN_RENDER_DISTANCE_CHUNKS: u32 = 2;
const MAX_RENDER_DISTANCE_CHUNKS: u32 = 128;
const MAX_CHUNK_UPLOADS_PER_FRAME: usize = 4;

pub fn run() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("event loop error");
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum UiMode {
    Playing,
    Paused,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MenuPage {
    Main,
    Settings,
    Graphics,
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

#[derive(Clone, Copy)]
struct ChunkBuildRequest {
    chunk: (i64, i64),
    version: u64,
}

struct ChunkBuildResult {
    chunk: (i64, i64),
    version: u64,
    vertices: Vec<Vertex>,
}

struct ChunkBuildPipeline {
    request_tx: mpsc::Sender<ChunkBuildRequest>,
    result_rx: mpsc::Receiver<ChunkBuildResult>,
}

impl ChunkBuildPipeline {
    fn new(world: World) -> Self {
        let worker_count = std::thread::available_parallelism()
            .map(|n| n.get().saturating_sub(1).clamp(1, 6))
            .unwrap_or(2);
        let (request_tx, request_rx) = mpsc::channel::<ChunkBuildRequest>();
        let (result_tx, result_rx) = mpsc::channel::<ChunkBuildResult>();
        let shared_rx = Arc::new(Mutex::new(request_rx));

        for worker_index in 0..worker_count {
            let rx = shared_rx.clone();
            let tx = result_tx.clone();
            let worker_world = world.clone();
            let thread_name = format!("chunk-mesh-worker-{worker_index}");
            std::thread::Builder::new()
                .name(thread_name)
                .spawn(move || {
                    loop {
                        let request = {
                            let guard = rx.lock().expect("chunk request lock poisoned");
                            guard.recv()
                        };
                        let request = match request {
                            Ok(request) => request,
                            Err(_) => break,
                        };
                        let vertices = worker_world.build_chunk_mesh(request.chunk);
                        if tx
                            .send(ChunkBuildResult {
                                chunk: request.chunk,
                                version: request.version,
                                vertices,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                })
                .expect("failed to spawn chunk mesh worker");
        }

        drop(result_tx);
        Self {
            request_tx,
            result_rx,
        }
    }
}

struct App {
    window: Option<Arc<Window>>,
    window_id: Option<WindowId>,
    gpu: Option<GpuState>,
    world: World,
    actors: ActorRoster,
    free_camera: Camera,
    camera_mode: CameraMode,
    ui_mode: UiMode,
    menu_page: MenuPage,
    menu_index: usize,
    input: InputState,
    debug_overlay: DebugOverlay,
    physics: PhysicsConfig,
    lens: CameraLens,
    frame_cap_index: usize,
    render_distance_chunks: u32,
    visible_chunks: HashSet<(i64, i64)>,
    resident_chunks: HashSet<(i64, i64)>,
    requested_chunks: HashSet<(i64, i64)>,
    dirty_chunks: HashSet<(i64, i64)>,
    chunk_versions: HashMap<(i64, i64), u64>,
    chunk_pipeline: ChunkBuildPipeline,
    inventory: Inventory,
    world_time_seconds: f32,
    last_chunk_center: (i64, i64),
    last_frame: Instant,
    next_frame_at: Instant,
    rng_state: u64,
}

impl App {
    fn new() -> Self {
        let world = World::generate(64, 32, 64);
        let lens = CameraLens::default();
        let actors = ActorRoster::new(world.spawn_point());
        let free_camera = actors.local_player().camera(lens);
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15);
        let pipeline = ChunkBuildPipeline::new(world.clone());

        Self {
            window: None,
            window_id: None,
            gpu: None,
            world,
            actors,
            free_camera,
            camera_mode: CameraMode::Player,
            ui_mode: UiMode::Playing,
            menu_page: MenuPage::Main,
            menu_index: 0,
            input: InputState::new(),
            debug_overlay: DebugOverlay::new(),
            physics: PhysicsConfig::default(),
            lens,
            frame_cap_index: DEFAULT_FRAME_CAP_INDEX,
            render_distance_chunks: DEFAULT_RENDER_DISTANCE_CHUNKS,
            visible_chunks: HashSet::new(),
            resident_chunks: HashSet::new(),
            requested_chunks: HashSet::new(),
            dirty_chunks: HashSet::new(),
            chunk_versions: HashMap::new(),
            chunk_pipeline: pipeline,
            inventory: Inventory::new(),
            world_time_seconds: 0.0,
            last_chunk_center: (0, 0),
            last_frame: Instant::now(),
            next_frame_at: Instant::now(),
            rng_state: seed,
        }
    }

    fn active_camera(&self) -> Camera {
        match self.camera_mode {
            CameraMode::Player => self.actors.local_player().camera(self.lens),
            CameraMode::Free => self.free_camera,
        }
    }

    fn sync_active_camera(&mut self) {
        let camera = self.active_camera();
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.set_camera(camera);
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

    fn refresh_window_title(&self) {
        if let Some(window) = &self.window {
            window.set_title(&format!(
                "Voxel Starter [{} | {} | RD {}]",
                self.frame_cap_label(),
                self.camera_mode.label(),
                self.render_distance_chunks
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
                self.free_camera = self.actors.local_player().camera(self.lens);
                CameraMode::Free
            }
            CameraMode::Free => CameraMode::Player,
        };
        self.sync_active_camera();
        self.refresh_window_title();
    }

    fn capture_mouse(&mut self) {
        if let Some(window) = &self.window {
            let _ = window
                .set_cursor_grab(CursorGrabMode::Locked)
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

    fn open_pause_menu(&mut self) {
        self.ui_mode = UiMode::Paused;
        self.menu_page = MenuPage::Main;
        self.menu_index = 0;
        self.release_mouse();
    }

    fn close_pause_menu(&mut self) {
        self.ui_mode = UiMode::Playing;
    }

    fn update(&mut self, dt: f32) {
        if self.ui_mode == UiMode::Paused {
            return;
        }
        match self.camera_mode {
            CameraMode::Player => self.update_player(dt),
            CameraMode::Free => self.update_free_camera(dt),
        }
        self.sync_active_camera();
    }

    fn update_player(&mut self, dt: f32) {
        let input = MovementInput {
            forward: axis_value(self.input.key(KeyCode::KeyW), self.input.key(KeyCode::KeyS)),
            strafe: axis_value(self.input.key(KeyCode::KeyD), self.input.key(KeyCode::KeyA)),
            jump_pressed: self.input.key(KeyCode::Space),
        };

        physics::update_player(
            self.actors.local_player_mut(),
            &input,
            &self.world,
            &self.physics,
            dt,
        );
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

        if self.world.is_out_of_bounds(
            self.free_camera.position - Vec3::Y * PLAYER_EYE_HEIGHT,
            self.physics.respawn_margin,
        ) {
            self.free_camera.position = self.world.spawn_point() + Vec3::Y * PLAYER_EYE_HEIGHT;
        }
    }

    fn current_chunk_center(&self) -> (i64, i64) {
        let cam = self.active_camera();
        let wx = cam.position.x.floor() as i64;
        let wz = cam.position.z.floor() as i64;
        World::world_to_chunk(wx, wz)
    }

    fn schedule_visible_chunks(&mut self) {
        let center = self.current_chunk_center();
        let render_distance = self.render_distance_chunks as i64;
        let radius_sq = render_distance * render_distance;
        let mut desired = Vec::new();
        for dz in -render_distance..=render_distance {
            for dx in -render_distance..=render_distance {
                if dx * dx + dz * dz > radius_sq {
                    continue;
                }
                desired.push((center.0 + dx, center.1 + dz));
            }
        }
        desired.sort_by_key(|(x, z)| {
            let dx = x - center.0;
            let dz = z - center.1;
            dx * dx + dz * dz
        });
        let desired_set: HashSet<(i64, i64)> = desired.iter().copied().collect();

        let chunks_to_remove: Vec<(i64, i64)> = self
            .visible_chunks
            .difference(&desired_set)
            .copied()
            .collect();
        if let Some(gpu) = self.gpu.as_mut() {
            for chunk in &chunks_to_remove {
                gpu.remove_chunk_mesh(*chunk);
            }
        }
        for chunk in &chunks_to_remove {
            self.resident_chunks.remove(chunk);
        }

        self.visible_chunks = desired_set;
        self.last_chunk_center = center;

        for chunk in desired {
            if self.resident_chunks.contains(&chunk) && !self.dirty_chunks.contains(&chunk) {
                continue;
            }
            self.ensure_chunk_requested(chunk);
        }
    }

    fn process_chunk_build_results(&mut self) {
        let mut uploads_remaining = MAX_CHUNK_UPLOADS_PER_FRAME;

        while uploads_remaining > 0 {
            let result = match self.chunk_pipeline.result_rx.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            };

            self.requested_chunks.remove(&result.chunk);
            let newest_version = self.chunk_versions.get(&result.chunk).copied().unwrap_or(0);
            if result.version != newest_version {
                self.ensure_chunk_requested(result.chunk);
                continue;
            }
            if !self.visible_chunks.contains(&result.chunk) {
                continue;
            }

            if let Some(gpu) = self.gpu.as_mut() {
                gpu.upsert_chunk_mesh(result.chunk, &result.vertices);
            }
            self.resident_chunks.insert(result.chunk);
            self.dirty_chunks.remove(&result.chunk);
            uploads_remaining -= 1;
        }
    }

    fn ensure_chunk_requested(&mut self, chunk: (i64, i64)) {
        if self.requested_chunks.contains(&chunk) {
            return;
        }
        let version = *self.chunk_versions.entry(chunk).or_insert(1);
        if self
            .chunk_pipeline
            .request_tx
            .send(ChunkBuildRequest { chunk, version })
            .is_ok()
        {
            self.requested_chunks.insert(chunk);
        }
    }

    fn mark_chunk_dirty(&mut self, chunk: (i64, i64)) {
        let version = self
            .chunk_versions
            .entry(chunk)
            .and_modify(|v| *v = v.saturating_add(1))
            .or_insert(1);
        self.dirty_chunks.insert(chunk);
        if self.visible_chunks.contains(&chunk) && !self.requested_chunks.contains(&chunk) {
            let _ = self.chunk_pipeline.request_tx.send(ChunkBuildRequest {
                chunk,
                version: *version,
            });
            self.requested_chunks.insert(chunk);
        }
    }

    fn mark_block_change_dirty(&mut self, x: i64, z: i64) {
        let chunk = World::world_to_chunk(x, z);
        for dz in -1..=1 {
            for dx in -1..=1 {
                self.mark_chunk_dirty((chunk.0 + dx, chunk.1 + dz));
            }
        }
    }

    fn respawn_player_random_near_center(&mut self) {
        let (center_x, center_z) = self.world.center_column();
        let radius = 50;
        let min_x = center_x - radius;
        let max_x = center_x + radius;
        let min_z = center_z - radius;
        let max_z = center_z + radius;

        let mut spawn = self.world.spawn_point();
        for _ in 0..128 {
            let x = random_i32_inclusive(&mut self.rng_state, min_x, max_x);
            let z = random_i32_inclusive(&mut self.rng_state, min_z, max_z);
            if let Some(candidate) = self.world.spawn_point_for_column(x, z) {
                spawn = candidate;
                break;
            }
        }

        self.actors.respawn_local_player(spawn);
        self.free_camera = self.actors.local_player().camera(self.lens);
        self.sync_active_camera();
    }

    fn handle_menu_navigation(&mut self, code: KeyCode) {
        let item_count = self.menu_item_count();
        match code {
            KeyCode::ArrowUp => {
                self.menu_index = if self.menu_index == 0 {
                    item_count - 1
                } else {
                    self.menu_index - 1
                };
            }
            KeyCode::ArrowDown => {
                self.menu_index = (self.menu_index + 1) % item_count;
            }
            KeyCode::ArrowLeft => {
                if self.menu_page == MenuPage::Graphics && self.menu_index == 0 {
                    self.adjust_render_distance(-1);
                }
            }
            KeyCode::ArrowRight => {
                if self.menu_page == MenuPage::Graphics && self.menu_index == 0 {
                    self.adjust_render_distance(1);
                }
            }
            KeyCode::Enter => {
                self.activate_menu_item();
            }
            KeyCode::Escape => {
                self.menu_back_or_resume();
            }
            _ => {}
        }
    }

    fn menu_item_count(&self) -> usize {
        match self.menu_page {
            MenuPage::Main => 4,
            MenuPage::Settings => 2,
            MenuPage::Graphics => 2,
        }
    }

    fn activate_menu_item(&mut self) {
        match self.menu_page {
            MenuPage::Main => match self.menu_index {
                0 => self.close_pause_menu(),
                1 => {
                    self.respawn_player_random_near_center();
                    self.close_pause_menu();
                }
                2 => {
                    self.menu_page = MenuPage::Settings;
                    self.menu_index = 0;
                }
                3 => {
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                    std::process::exit(0);
                }
                _ => {}
            },
            MenuPage::Settings => match self.menu_index {
                0 => {
                    self.menu_page = MenuPage::Graphics;
                    self.menu_index = 0;
                }
                1 => {
                    self.menu_page = MenuPage::Main;
                    self.menu_index = 0;
                }
                _ => {}
            },
            MenuPage::Graphics => match self.menu_index {
                0 => {
                    self.adjust_render_distance(1);
                }
                1 => {
                    self.menu_page = MenuPage::Settings;
                    self.menu_index = 0;
                }
                _ => {}
            },
        }
    }

    fn menu_back_or_resume(&mut self) {
        match self.menu_page {
            MenuPage::Main => self.close_pause_menu(),
            MenuPage::Settings => {
                self.menu_page = MenuPage::Main;
                self.menu_index = 0;
            }
            MenuPage::Graphics => {
                self.menu_page = MenuPage::Settings;
                self.menu_index = 0;
            }
        }
    }

    fn adjust_render_distance(&mut self, delta: i32) {
        let value = self.render_distance_chunks as i32 + delta;
        self.render_distance_chunks = value
            .clamp(
                MIN_RENDER_DISTANCE_CHUNKS as i32,
                MAX_RENDER_DISTANCE_CHUNKS as i32,
            ) as u32;
        self.schedule_visible_chunks();
        self.refresh_window_title();
    }

    fn menu_overlay(&self) -> (String, Vec<String>, usize) {
        match self.menu_page {
            MenuPage::Main => (
                "PAUSED".to_string(),
                vec![
                    "RESUME".to_string(),
                    "RESPAWN".to_string(),
                    "SETTINGS".to_string(),
                    "QUIT TO DESKTOP".to_string(),
                ],
                self.menu_index,
            ),
            MenuPage::Settings => (
                "SETTINGS".to_string(),
                vec!["GRAPHICS".to_string(), "BACK".to_string()],
                self.menu_index,
            ),
            MenuPage::Graphics => (
                "GRAPHICS".to_string(),
                vec![
                    format!("RENDER DISTANCE {}", self.render_distance_chunks),
                    "BACK".to_string(),
                ],
                self.menu_index,
            ),
        }
    }

    fn hud_lines(&self) -> Vec<String> {
        let slot = self.inventory.selected_slot();
        let slots = self.inventory.slots();
        vec![
            format!("SUN {:.1}", self.world_time_seconds),
            format!("SEL {} {}", block_label(slot.block), slot.count),
            format!(
                "G {} D {} S {}",
                slots[0].count, slots[1].count, slots[2].count
            ),
        ]
    }

    fn sky_body_overlay(&self) -> Vec<OverlayVertex> {
        let mut vertices = Vec::new();
        let Some(window) = &self.window else {
            return vertices;
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return vertices;
        }
        let camera = self.active_camera();
        let celestial = celestial_state_for_time(self.world_time_seconds);

        if let Some((x, y)) = direction_to_screen(
            camera,
            celestial.sun_direction,
            size.width as f32,
            size.height as f32,
        ) {
            let glow = 16.0 + 22.0 * celestial.sun_intensity;
            let core = 5.0 + 7.0 * celestial.sun_intensity;
            push_circle(&mut vertices, x, y, glow, [1.0, 0.62, 0.34, 0.24], 18);
            push_circle(&mut vertices, x, y, core, [1.0, 0.90, 0.72, 0.95], 14);
        }
        if let Some((x, y)) = direction_to_screen(
            camera,
            celestial.moon_direction,
            size.width as f32,
            size.height as f32,
        ) {
            let glow = 9.0 + 10.0 * celestial.moon_intensity;
            let core = 3.0 + 4.0 * celestial.moon_intensity;
            push_circle(&mut vertices, x, y, glow, [0.72, 0.80, 1.0, 0.18], 14);
            push_circle(&mut vertices, x, y, core, [0.90, 0.94, 1.0, 0.85], 12);
        }

        vertices
    }

    fn edit_block_from_click(&mut self, remove: bool) {
        if self.ui_mode != UiMode::Playing {
            return;
        }
        let camera = self.active_camera();
        let Some((hit, previous)) =
            raycast_world(&self.world, camera.position, camera.forward(), 7.0, 0.05)
        else {
            return;
        };

        if remove {
            let block = self.world.block_at_i64(hit.0, hit.1, hit.2);
            if matches!(block, Block::Air) {
                return;
            }
            self.world.set_block_i64(hit.0, hit.1, hit.2, Block::Air);
            self.inventory.add_block(block);
            self.mark_block_change_dirty(hit.0, hit.2);
            return;
        }

        let place = previous;
        if place.1 <= 0 || place.1 >= 63 {
            return;
        }

        let player_pos = self.actors.local_player().motion.position;
        let block_min = Vec3::new(place.0 as f32, place.1 as f32, place.2 as f32);
        let block_max = block_min + Vec3::ONE;
        let player_min = Vec3::new(player_pos.x - 0.32, player_pos.y, player_pos.z - 0.32);
        let player_max = Vec3::new(player_pos.x + 0.32, player_pos.y + 1.8, player_pos.z + 0.32);
        let intersects = block_min.x < player_max.x
            && block_max.x > player_min.x
            && block_min.y < player_max.y
            && block_max.y > player_min.y
            && block_min.z < player_max.z
            && block_max.z > player_min.z;
        if intersects {
            return;
        }

        if !matches!(self.world.block_at_i64(place.0, place.1, place.2), Block::Air) {
            return;
        }

        let Some(block_to_place) = self.inventory.try_take_selected() else {
            return;
        };

        self.world
            .set_block_i64(place.0, place.1, place.2, block_to_place);
        self.mark_block_change_dirty(place.0, place.2);
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
        self.lens = self
            .lens
            .with_aspect(size.width.max(1) as f32 / size.height.max(1) as f32);
        self.free_camera = self.actors.local_player().camera(self.lens);
        let gpu = pollster::block_on(GpuState::new(window.clone(), self.active_camera()));

        self.window_id = Some(window.id());
        self.window = Some(window);
        self.gpu = Some(gpu);
        self.last_frame = Instant::now();
        self.next_frame_at = self.last_frame;
        self.last_chunk_center = self.current_chunk_center();
        self.schedule_visible_chunks();
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
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.resize(size);
                }
                self.lens = self
                    .lens
                    .with_aspect(size.width.max(1) as f32 / size.height.max(1) as f32);
                self.sync_active_camera();
            }
            WindowEvent::RedrawRequested => {
                let menu_overlay = if self.ui_mode == UiMode::Paused {
                    let (title, lines, selected) = self.menu_overlay();
                    Some(
                        self.debug_overlay
                            .build_menu_vertices(&title, &lines, selected),
                    )
                } else {
                    None
                };
                let hud_lines = self.hud_lines();
                let sky_overlay = self.sky_body_overlay();
                if let Some(gpu) = self.gpu.as_mut() {
                    let mut overlay_vertices = sky_overlay;
                    overlay_vertices.extend(
                        self.debug_overlay
                            .build_vertices(gpu.estimated_gpu_memory_bytes(), &hud_lines),
                    );
                    if let Some(menu_vertices) = menu_overlay {
                        overlay_vertices.extend(menu_vertices);
                    }
                    match gpu.render(&overlay_vertices) {
                        RenderOutcome::Success | RenderOutcome::SkipFrame => {}
                        RenderOutcome::Reconfigure => {
                            if let Some(window) = &self.window {
                                gpu.resize(window.inner_size());
                            }
                        }
                        RenderOutcome::FatalSurfaceLoss => event_loop.exit(),
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => {
                            self.input.pressed.insert(code);

                            if self.ui_mode == UiMode::Paused {
                                self.handle_menu_navigation(code);
                                return;
                            }

                            if code == KeyCode::F3 && !event.repeat {
                                self.debug_overlay.visible = !self.debug_overlay.visible;
                            }
                            if code == KeyCode::F4 && !event.repeat {
                                self.cycle_frame_cap();
                            }
                            if code == KeyCode::F5 && !event.repeat {
                                self.toggle_camera_mode();
                            }
                            if code == KeyCode::Escape && !event.repeat {
                                self.open_pause_menu();
                            }
                            if code == KeyCode::Digit1 && !event.repeat {
                                self.inventory.select_index(0);
                            }
                            if code == KeyCode::Digit2 && !event.repeat {
                                self.inventory.select_index(1);
                            }
                            if code == KeyCode::Digit3 && !event.repeat {
                                self.inventory.select_index(2);
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
                button,
                ..
            } => {
                if self.ui_mode == UiMode::Paused {
                    return;
                }
                if !self.input.mouse_captured && button == MouseButton::Left {
                    self.capture_mouse();
                    return;
                }
                if self.input.mouse_captured {
                    match button {
                        MouseButton::Left => self.edit_block_from_click(true),
                        MouseButton::Right => self.edit_block_from_click(false),
                        _ => {}
                    }
                }
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
        if !self.input.mouse_captured || self.ui_mode == UiMode::Paused {
            return;
        }

        if let DeviceEvent::MouseMotion { delta } = event {
            match self.camera_mode {
                CameraMode::Player => {
                    let actor = self.actors.local_player_mut();
                    actor.look.yaw += delta.0 as f32 * LOOK_SENSITIVITY;
                    actor.look.pitch -= delta.1 as f32 * LOOK_SENSITIVITY;
                    actor.look.pitch = actor.look.pitch.clamp(-MAX_PITCH, MAX_PITCH);
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
        self.world_time_seconds += dt;
        self.debug_overlay.record_frame(dt);
        self.update(dt);
        let center = self.current_chunk_center();
        if center != self.last_chunk_center {
            self.schedule_visible_chunks();
        }
        self.process_chunk_build_results();
        let camera = self.active_camera();
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.set_environment(
                self.world_time_seconds,
                camera.position,
                self.world.terrain_seed(),
            );
        }

        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn axis_value(positive: bool, negative: bool) -> f32 {
    positive as i8 as f32 - negative as i8 as f32
}

fn block_label(block: Block) -> &'static str {
    match block {
        Block::Air => "AIR",
        Block::Grass => "GRASS",
        Block::Dirt => "DIRT",
        Block::Stone => "STONE",
    }
}

fn random_i32_inclusive(state: &mut u64, min: i32, max: i32) -> i32 {
    *state ^= *state >> 12;
    *state ^= *state << 25;
    *state ^= *state >> 27;
    let value = state.wrapping_mul(0x2545F4914F6CDD1D);

    if max <= min {
        return min;
    }

    let span = (max - min + 1) as u32;
    min + (value as u32 % span) as i32
}

fn raycast_world(
    world: &World,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
    step: f32,
) -> Option<((i64, i32, i64), (i64, i32, i64))> {
    let dir = direction.normalize_or_zero();
    if dir.length_squared() <= f32::EPSILON {
        return None;
    }

    let mut previous = voxel_coords(origin);
    let mut distance = 0.0;
    while distance <= max_distance {
        let point = origin + dir * distance;
        let cell = voxel_coords(point);
        if cell != previous {
            if world.is_solid_i64(cell.0, cell.1, cell.2) {
                return Some((cell, previous));
            }
            previous = cell;
        }
        distance += step;
    }

    None
}

fn voxel_coords(point: Vec3) -> (i64, i32, i64) {
    (
        point.x.floor() as i64,
        point.y.floor() as i32,
        point.z.floor() as i64,
    )
}

fn direction_to_screen(
    camera: Camera,
    direction: Vec3,
    width: f32,
    height: f32,
) -> Option<(f32, f32)> {
    let world_point = camera.position + direction.normalize_or_zero() * 1000.0;
    let clip = camera.view_proj() * world_point.extend(1.0);
    if clip.w <= 0.0 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    if ndc.z < 0.0 || ndc.z > 1.0 {
        return None;
    }
    if ndc.x.abs() > 1.2 || ndc.y.abs() > 1.2 {
        return None;
    }

    let x = (ndc.x * 0.5 + 0.5) * width;
    let y = (1.0 - (ndc.y * 0.5 + 0.5)) * height;
    Some((x, y))
}

fn push_circle(
    vertices: &mut Vec<OverlayVertex>,
    cx: f32,
    cy: f32,
    radius: f32,
    color: [f32; 4],
    segments: usize,
) {
    if segments < 3 || radius <= 0.0 {
        return;
    }
    let step = std::f32::consts::TAU / segments as f32;
    for i in 0..segments {
        let a0 = i as f32 * step;
        let a1 = (i + 1) as f32 * step;
        let p0 = [cx + a0.cos() * radius, cy + a0.sin() * radius];
        let p1 = [cx + a1.cos() * radius, cy + a1.sin() * radius];
        vertices.extend_from_slice(&[
            OverlayVertex {
                position: [cx, cy],
                color,
            },
            OverlayVertex {
                position: p0,
                color,
            },
            OverlayVertex {
                position: p1,
                color,
            },
        ]);
    }
}
