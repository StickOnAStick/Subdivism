use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
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
        inventory::{BACKPACK_SIZE, HOTBAR_SIZE, Inventory},
        physics::{self, MovementInput, PhysicsConfig},
        terrain_recipe::TerrainRecipe,
        world::{Block, CHUNK_SIZE, DEFAULT_WORLD_SEED, TerrainConfig, World},
    },
    mesh::Vertex,
    render::{ChunkRenderKey, GpuState, RenderOutcome, celestial_state_for_time},
};

const FREE_CAMERA_SPEED: f32 = 10.0;
const LOOK_SENSITIVITY: f32 = 0.0025;
const MAX_PITCH: f32 = 1.54;
const FRAME_CAP_PRESETS: [Option<u32>; 5] = [None, Some(60), Some(120), Some(144), Some(240)];
const DEFAULT_FRAME_CAP_INDEX: usize = 0;
const DEFAULT_RENDER_DISTANCE_CHUNKS: u32 = 24;
const MIN_RENDER_DISTANCE_CHUNKS: u32 = 2;
const MAX_RENDER_DISTANCE_CHUNKS: u32 = 128;
const MAX_CHUNK_UPLOADS_PER_FRAME: usize = 8;
const FULL_DETAIL_RADIUS_CHUNKS: i64 = 16;
const MID_DETAIL_RADIUS_CHUNKS: i64 = 32;
const LOW_DETAIL_RADIUS_CHUNKS: i64 = 64;

pub fn run() {
    let options = AppLaunchOptions::from_env();
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let mut app = App::new(options);
    event_loop.run_app(&mut app).expect("event loop error");
}

#[derive(Clone, Debug)]
struct AppLaunchOptions {
    seed_override: Option<i64>,
    terrain_file: Option<PathBuf>,
    dev_mode: bool,
}

impl AppLaunchOptions {
    fn from_env() -> Self {
        let mut seed_override = None;
        let mut terrain_file = None;
        let mut dev_mode = false;
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--seed" => {
                    if let Some(raw) = args.next() {
                        if let Ok(seed) = raw.parse::<i64>() {
                            seed_override = Some(seed);
                        } else {
                            eprintln!("ignoring invalid --seed value: {raw}");
                        }
                    }
                }
                "--terrain-file" => {
                    if let Some(path) = args.next() {
                        terrain_file = Some(PathBuf::from(path));
                    }
                }
                "--dev" | "--dev-mode" => {
                    dev_mode = true;
                }
                _ => {}
            }
        }
        Self {
            seed_override,
            terrain_file,
            dev_mode,
        }
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum UiMode {
    Playing,
    Paused,
    Inventory,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MenuPage {
    Main,
    Settings,
    Graphics,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InventorySection {
    Hotbar,
    Backpack,
}

#[derive(Clone, Copy)]
struct InventoryCursor {
    section: InventorySection,
    index: usize,
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
    key: ChunkRenderKey,
    version: u64,
}

struct ChunkBuildResult {
    key: ChunkRenderKey,
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
                        let vertices = worker_world
                            .build_chunk_mesh_lod(request.key.origin_chunk, request.key.lod_level);
                        if tx
                            .send(ChunkBuildResult {
                                key: request.key,
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
    inventory_cursor: InventoryCursor,
    input: InputState,
    debug_overlay: DebugOverlay,
    physics: PhysicsConfig,
    lens: CameraLens,
    frame_cap_index: usize,
    render_distance_chunks: u32,
    visible_chunks: HashSet<ChunkRenderKey>,
    resident_chunks: HashSet<ChunkRenderKey>,
    requested_chunks: HashSet<ChunkRenderKey>,
    dirty_chunks: HashSet<ChunkRenderKey>,
    chunk_versions: HashMap<ChunkRenderKey, u64>,
    chunk_pipeline: ChunkBuildPipeline,
    inventory: Inventory,
    world_seed: i64,
    terrain_profile: String,
    terrain_recipe_path: Option<PathBuf>,
    dev_mode: bool,
    world_time_seconds: f32,
    last_chunk_center: (i64, i64),
    last_frame: Instant,
    next_frame_at: Instant,
    rng_state: u64,
}

impl App {
    fn new(options: AppLaunchOptions) -> Self {
        let mut terrain = TerrainConfig::balanced();
        let mut world_seed = options.seed_override.unwrap_or(DEFAULT_WORLD_SEED);
        let mut terrain_profile = "balanced".to_string();
        let mut terrain_recipe_path = None;
        if let Some(path) = options.terrain_file.as_ref() {
            match TerrainRecipe::from_file(path) {
                Ok(recipe) => {
                    terrain = recipe.terrain;
                    terrain_profile = recipe.profile;
                    world_seed = options.seed_override.unwrap_or(recipe.seed);
                    terrain_recipe_path = Some(path.clone());
                }
                Err(err) => {
                    eprintln!("failed to load terrain recipe {}: {err}", path.display());
                }
            }
        }

        let world = World::generate_with_terrain_and_seed(terrain, world_seed);
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
            inventory_cursor: InventoryCursor {
                section: InventorySection::Hotbar,
                index: 0,
            },
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
            world_seed,
            terrain_profile,
            terrain_recipe_path,
            dev_mode: options.dev_mode,
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

    fn sync_lens_to_cameras(&mut self) {
        self.free_camera.lens = self.lens;
        self.sync_active_camera();
    }

    fn update_far_plane_for_render_distance(&mut self) {
        let target_far = ((self.render_distance_chunks as f32 + 2.0) * CHUNK_SIZE as f32 * 2.0)
            .clamp(500.0, 8192.0);
        self.lens.z_far = target_far;
        self.sync_lens_to_cameras();
    }

    fn replace_world(
        &mut self,
        world: World,
        world_seed: i64,
        terrain_profile: String,
        terrain_recipe_path: Option<PathBuf>,
    ) {
        let mut existing_keys = self.visible_chunks.clone();
        existing_keys.extend(self.resident_chunks.iter().copied());
        if let Some(gpu) = self.gpu.as_mut() {
            for key in existing_keys {
                gpu.remove_chunk_mesh(key);
            }
        }

        self.world = world;
        self.world_seed = world_seed;
        self.terrain_profile = terrain_profile;
        self.terrain_recipe_path = terrain_recipe_path;
        self.chunk_pipeline = ChunkBuildPipeline::new(self.world.clone());
        self.visible_chunks.clear();
        self.resident_chunks.clear();
        self.requested_chunks.clear();
        self.dirty_chunks.clear();
        self.chunk_versions.clear();

        self.actors.respawn_local_player(self.world.spawn_point());
        self.free_camera = self.actors.local_player().camera(self.lens);
        self.last_chunk_center = self.current_chunk_center();
        self.schedule_visible_chunks();
        self.sync_active_camera();
        self.refresh_window_title();
    }

    fn reload_dev_world(&mut self) {
        if !self.dev_mode {
            return;
        }
        let mut terrain = self.world.terrain_config();
        let mut seed = self.world_seed;
        let mut profile = self.terrain_profile.clone();
        let recipe_path = self.terrain_recipe_path.clone();

        if let Some(path) = recipe_path.as_ref() {
            match TerrainRecipe::from_file(path) {
                Ok(recipe) => {
                    terrain = recipe.terrain;
                    seed = recipe.seed;
                    profile = recipe.profile;
                }
                Err(err) => {
                    eprintln!("failed to reload terrain recipe {}: {err}", path.display());
                    return;
                }
            }
        }

        let world = World::generate_with_terrain_and_seed(terrain, seed);
        self.replace_world(world, seed, profile, recipe_path);
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
            let dev_flag = if self.dev_mode { " DEV" } else { "" };
            window.set_title(&format!(
                "Voxel Starter [{} | {} | RD {} | SEED {}{}]",
                self.frame_cap_label(),
                self.camera_mode.label(),
                self.render_distance_chunks,
                self.world_seed,
                dev_flag
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

    fn open_inventory(&mut self) {
        self.ui_mode = UiMode::Inventory;
        self.inventory_cursor = InventoryCursor {
            section: InventorySection::Hotbar,
            index: self.inventory.selected_hotbar_index(),
        };
        self.release_mouse();
    }

    fn close_inventory(&mut self) {
        self.ui_mode = UiMode::Playing;
    }

    fn update(&mut self, dt: f32) {
        if self.ui_mode != UiMode::Playing {
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
            sprint_held: self.input.key(KeyCode::ShiftLeft) || self.input.key(KeyCode::ShiftRight),
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
        let render_distance =
            (self.render_distance_chunks as i64).clamp(1, MAX_RENDER_DISTANCE_CHUNKS as i64);
        let mut desired = Vec::new();
        collect_lod_ring(
            &mut desired,
            center,
            0,
            render_distance.min(FULL_DETAIL_RADIUS_CHUNKS),
            0,
        );
        if render_distance > FULL_DETAIL_RADIUS_CHUNKS {
            collect_lod_ring(
                &mut desired,
                center,
                FULL_DETAIL_RADIUS_CHUNKS,
                render_distance.min(MID_DETAIL_RADIUS_CHUNKS),
                1,
            );
        }
        if render_distance > MID_DETAIL_RADIUS_CHUNKS {
            collect_lod_ring(
                &mut desired,
                center,
                MID_DETAIL_RADIUS_CHUNKS,
                render_distance.min(LOW_DETAIL_RADIUS_CHUNKS),
                2,
            );
        }
        if render_distance > LOW_DETAIL_RADIUS_CHUNKS {
            collect_lod_ring(
                &mut desired,
                center,
                LOW_DETAIL_RADIUS_CHUNKS,
                render_distance,
                3,
            );
        }

        desired.sort_by_key(|key| {
            let dx = key.origin_chunk.0 - center.0;
            let dz = key.origin_chunk.1 - center.1;
            (dx * dx + dz * dz, key.lod_level)
        });
        let desired_set: HashSet<ChunkRenderKey> = desired.iter().copied().collect();

        let chunks_to_remove: Vec<ChunkRenderKey> = self
            .visible_chunks
            .difference(&desired_set)
            .copied()
            .collect();
        if let Some(gpu) = self.gpu.as_mut() {
            for key in &chunks_to_remove {
                gpu.remove_chunk_mesh(*key);
            }
        }
        for key in &chunks_to_remove {
            self.resident_chunks.remove(key);
        }

        self.visible_chunks = desired_set;
        self.last_chunk_center = center;

        for key in desired {
            if self.resident_chunks.contains(&key) && !self.dirty_chunks.contains(&key) {
                continue;
            }
            self.ensure_chunk_requested(key);
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

            self.requested_chunks.remove(&result.key);
            let newest_version = self.chunk_versions.get(&result.key).copied().unwrap_or(0);
            if result.version != newest_version {
                self.ensure_chunk_requested(result.key);
                continue;
            }
            if !self.visible_chunks.contains(&result.key) {
                continue;
            }

            if let Some(gpu) = self.gpu.as_mut() {
                gpu.upsert_chunk_mesh(result.key, &result.vertices);
            }
            self.resident_chunks.insert(result.key);
            self.dirty_chunks.remove(&result.key);
            uploads_remaining -= 1;
        }
    }

    fn ensure_chunk_requested(&mut self, key: ChunkRenderKey) {
        if self.requested_chunks.contains(&key) {
            return;
        }
        let version = *self.chunk_versions.entry(key).or_insert(1);
        if self
            .chunk_pipeline
            .request_tx
            .send(ChunkBuildRequest { key, version })
            .is_ok()
        {
            self.requested_chunks.insert(key);
        }
    }

    fn mark_chunk_dirty(&mut self, key: ChunkRenderKey) {
        let version = self
            .chunk_versions
            .entry(key)
            .and_modify(|v| *v = v.saturating_add(1))
            .or_insert(1);
        self.dirty_chunks.insert(key);
        if self.visible_chunks.contains(&key) && !self.requested_chunks.contains(&key) {
            let _ = self.chunk_pipeline.request_tx.send(ChunkBuildRequest {
                key,
                version: *version,
            });
            self.requested_chunks.insert(key);
        }
    }

    fn mark_block_change_dirty(&mut self, x: i64, z: i64) {
        let chunk = World::world_to_chunk(x, z);
        for dz in -1..=1 {
            for dx in -1..=1 {
                self.mark_chunk_dirty(ChunkRenderKey {
                    origin_chunk: (chunk.0 + dx, chunk.1 + dz),
                    lod_level: 0,
                });
            }
        }
        for lod_level in 1..=3 {
            let step = 1_i64 << lod_level;
            self.mark_chunk_dirty(ChunkRenderKey {
                origin_chunk: aligned_origin(chunk, step),
                lod_level,
            });
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

    fn handle_inventory_navigation(&mut self, code: KeyCode) {
        match code {
            KeyCode::ArrowUp => {
                self.inventory_cursor.section = InventorySection::Hotbar;
                self.inventory_cursor.index = self
                    .inventory_cursor
                    .index
                    .min(self.inventory_section_len(InventorySection::Hotbar) - 1);
            }
            KeyCode::ArrowDown => {
                self.inventory_cursor.section = InventorySection::Backpack;
                self.inventory_cursor.index = self
                    .inventory_cursor
                    .index
                    .min(self.inventory_section_len(InventorySection::Backpack) - 1);
            }
            KeyCode::ArrowLeft => {
                let len = self.inventory_section_len(self.inventory_cursor.section);
                self.inventory_cursor.index = if self.inventory_cursor.index == 0 {
                    len - 1
                } else {
                    self.inventory_cursor.index - 1
                };
            }
            KeyCode::ArrowRight => {
                let len = self.inventory_section_len(self.inventory_cursor.section);
                self.inventory_cursor.index = (self.inventory_cursor.index + 1) % len;
            }
            KeyCode::Enter | KeyCode::Space => {
                self.activate_inventory_selection();
            }
            KeyCode::Escape | KeyCode::KeyE => {
                self.close_inventory();
            }
            _ => {}
        }
    }

    fn inventory_section_len(&self, section: InventorySection) -> usize {
        match section {
            InventorySection::Hotbar => HOTBAR_SIZE,
            InventorySection::Backpack => BACKPACK_SIZE,
        }
    }

    fn activate_inventory_selection(&mut self) {
        match self.inventory_cursor.section {
            InventorySection::Hotbar => {
                self.inventory.select_index(self.inventory_cursor.index);
            }
            InventorySection::Backpack => {
                let hotbar_index = self.inventory.selected_hotbar_index();
                self.inventory
                    .move_backpack_slot_to_hotbar(self.inventory_cursor.index, hotbar_index);
            }
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
        self.render_distance_chunks = value.clamp(
            MIN_RENDER_DISTANCE_CHUNKS as i32,
            MAX_RENDER_DISTANCE_CHUNKS as i32,
        ) as u32;
        self.update_far_plane_for_render_distance();
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

    fn inventory_overlay(&self) -> (String, Vec<String>, usize) {
        let selected_hotbar = self.inventory.selected_hotbar_index();
        let hotbar_slot = self.inventory.hotbar()[selected_hotbar];
        let backpack_index = self.inventory_cursor.index.min(BACKPACK_SIZE - 1);
        let backpack_slot = self
            .inventory
            .backpack_slot(backpack_index)
            .unwrap_or_default();
        let selected_line = match self.inventory_cursor.section {
            InventorySection::Hotbar => 1,
            InventorySection::Backpack => 2,
        };

        (
            "INVENTORY".to_string(),
            vec![
                "ARROWS MOVE ENTER APPLY".to_string(),
                format!(
                    "HOTBAR SLOT {} {} {}",
                    selected_hotbar + 1,
                    block_label(hotbar_slot.block),
                    hotbar_slot.count
                ),
                format!(
                    "BACKPACK SLOT {} {} {}",
                    backpack_index + 1,
                    block_label(backpack_slot.block),
                    backpack_slot.count
                ),
                format!("ACTIVE HOTBAR {}", selected_hotbar + 1),
                "E OR ESC CLOSE".to_string(),
            ],
            selected_line,
        )
    }

    fn hud_lines(&self) -> Vec<String> {
        let slot = self.inventory.selected_slot();
        let hotbar = self.inventory.hotbar();
        let mut lines = vec![
            format!("SUN {:.1}", self.world_time_seconds),
            format!("SEED {}", self.world_seed),
            format!(
                "SLOT {} {} {}",
                self.inventory.selected_hotbar_index() + 1,
                block_label(slot.block),
                slot.count
            ),
            format!(
                "HB1 {} HB2 {} HB3 {}",
                hotbar[0].count, hotbar[1].count, hotbar[2].count
            ),
            format!(
                "INV {} OF {}",
                self.inventory.filled_slots(),
                self.inventory.slot_capacity()
            ),
        ];
        if self.dev_mode {
            lines.push(format!(
                "DEV PROFILE {}",
                self.terrain_profile.to_uppercase()
            ));
            if self.terrain_recipe_path.is_some() {
                lines.push("DEV FILE ON".to_string());
                lines.push("F6 RELOAD".to_string());
            }
        }
        lines
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
            if !self.inventory.add_block(block) {
                return;
            }
            self.world.set_block_i64(hit.0, hit.1, hit.2, Block::Air);
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

        if !matches!(
            self.world.block_at_i64(place.0, place.1, place.2),
            Block::Air
        ) {
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
        self.update_far_plane_for_render_distance();
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
                self.sync_lens_to_cameras();
            }
            WindowEvent::RedrawRequested => {
                let menu_overlay = match self.ui_mode {
                    UiMode::Paused => {
                        let (title, lines, selected) = self.menu_overlay();
                        Some(
                            self.debug_overlay
                                .build_menu_vertices(&title, &lines, selected),
                        )
                    }
                    UiMode::Inventory => {
                        let (title, lines, selected) = self.inventory_overlay();
                        Some(
                            self.debug_overlay
                                .build_menu_vertices(&title, &lines, selected),
                        )
                    }
                    UiMode::Playing => None,
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
                            if self.ui_mode == UiMode::Inventory {
                                self.handle_inventory_navigation(code);
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
                            if code == KeyCode::F6 && !event.repeat {
                                self.reload_dev_world();
                            }
                            if code == KeyCode::Escape && !event.repeat {
                                self.open_pause_menu();
                            }
                            if code == KeyCode::KeyE && !event.repeat {
                                self.open_inventory();
                            }
                            if let Some(index) = digit_to_hotbar_index(code) {
                                if !event.repeat {
                                    self.inventory.select_index(index);
                                }
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
                if self.ui_mode != UiMode::Playing {
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
        if !self.input.mouse_captured || self.ui_mode != UiMode::Playing {
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

fn digit_to_hotbar_index(code: KeyCode) -> Option<usize> {
    match code {
        KeyCode::Digit1 => Some(0),
        KeyCode::Digit2 => Some(1),
        KeyCode::Digit3 => Some(2),
        KeyCode::Digit4 => Some(3),
        KeyCode::Digit5 => Some(4),
        KeyCode::Digit6 => Some(5),
        KeyCode::Digit7 => Some(6),
        KeyCode::Digit8 => Some(7),
        KeyCode::Digit9 => Some(8),
        _ => None,
    }
}

fn aligned_origin(chunk: (i64, i64), step_chunks: i64) -> (i64, i64) {
    (
        chunk.0.div_euclid(step_chunks) * step_chunks,
        chunk.1.div_euclid(step_chunks) * step_chunks,
    )
}

fn collect_lod_ring(
    out: &mut Vec<ChunkRenderKey>,
    center_chunk: (i64, i64),
    min_radius: i64,
    max_radius: i64,
    lod_level: u8,
) {
    if max_radius <= 0 || max_radius <= min_radius {
        return;
    }
    let step = 1_i64 << lod_level;
    let min_sq = (min_radius * min_radius) as f32;
    let max_sq = (max_radius * max_radius) as f32;
    let start_x = (center_chunk.0 - max_radius).div_euclid(step) * step;
    let end_x = (center_chunk.0 + max_radius).div_euclid(step) * step;
    let start_z = (center_chunk.1 - max_radius).div_euclid(step) * step;
    let end_z = (center_chunk.1 + max_radius).div_euclid(step) * step;
    let center_x = center_chunk.0 as f32;
    let center_z = center_chunk.1 as f32;
    let step_f = step as f32;

    let mut origin_z = start_z;
    while origin_z <= end_z {
        let mut origin_x = start_x;
        while origin_x <= end_x {
            let tile_min_x = origin_x as f32;
            let tile_min_z = origin_z as f32;
            let tile_max_x = tile_min_x + step_f;
            let tile_max_z = tile_min_z + step_f;

            let nearest_x = center_x.clamp(tile_min_x, tile_max_x);
            let nearest_z = center_z.clamp(tile_min_z, tile_max_z);
            let min_dx = nearest_x - center_x;
            let min_dz = nearest_z - center_z;
            let tile_min_dist_sq = min_dx * min_dx + min_dz * min_dz;

            let farthest_x = if (center_x - tile_min_x).abs() > (center_x - tile_max_x).abs() {
                tile_min_x
            } else {
                tile_max_x
            };
            let farthest_z = if (center_z - tile_min_z).abs() > (center_z - tile_max_z).abs() {
                tile_min_z
            } else {
                tile_max_z
            };
            let max_dx = farthest_x - center_x;
            let max_dz = farthest_z - center_z;
            let tile_max_dist_sq = max_dx * max_dx + max_dz * max_dz;

            if tile_min_dist_sq <= max_sq && (min_radius == 0 || tile_max_dist_sq > min_sq) {
                out.push(ChunkRenderKey {
                    origin_chunk: (origin_x, origin_z),
                    lod_level,
                });
            }
            origin_x += step;
        }
        origin_z += step;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lod_selection_covers_entire_render_circle() {
        let center = (0_i64, 0_i64);
        for render_distance in [8_i64, 16, 24, 32, 48, 64, 96, 128] {
            let keys = collect_visible_keys_for_test(center, render_distance);
            for z in (center.1 - render_distance)..=(center.1 + render_distance) {
                for x in (center.0 - render_distance)..=(center.0 + render_distance) {
                    let dx = x as f32 - center.0 as f32;
                    let dz = z as f32 - center.1 as f32;
                    if dx * dx + dz * dz > (render_distance * render_distance) as f32 {
                        continue;
                    }
                    assert!(
                        chunk_covered_by_any_key((x, z), &keys),
                        "missing coverage at chunk ({x}, {z}) for render distance {render_distance}"
                    );
                }
            }
        }
    }

    fn collect_visible_keys_for_test(
        center: (i64, i64),
        render_distance: i64,
    ) -> Vec<ChunkRenderKey> {
        let mut desired = Vec::new();
        collect_lod_ring(
            &mut desired,
            center,
            0,
            render_distance.min(FULL_DETAIL_RADIUS_CHUNKS),
            0,
        );
        if render_distance > FULL_DETAIL_RADIUS_CHUNKS {
            collect_lod_ring(
                &mut desired,
                center,
                FULL_DETAIL_RADIUS_CHUNKS,
                render_distance.min(MID_DETAIL_RADIUS_CHUNKS),
                1,
            );
        }
        if render_distance > MID_DETAIL_RADIUS_CHUNKS {
            collect_lod_ring(
                &mut desired,
                center,
                MID_DETAIL_RADIUS_CHUNKS,
                render_distance.min(LOW_DETAIL_RADIUS_CHUNKS),
                2,
            );
        }
        if render_distance > LOW_DETAIL_RADIUS_CHUNKS {
            collect_lod_ring(
                &mut desired,
                center,
                LOW_DETAIL_RADIUS_CHUNKS,
                render_distance,
                3,
            );
        }
        desired
    }

    fn chunk_covered_by_any_key(chunk: (i64, i64), keys: &[ChunkRenderKey]) -> bool {
        keys.iter().any(|key| {
            let span = 1_i64 << key.lod_level;
            let min_x = key.origin_chunk.0;
            let min_z = key.origin_chunk.1;
            let max_x = min_x + span - 1;
            let max_z = min_z + span - 1;
            (min_x..=max_x).contains(&chunk.0) && (min_z..=max_z).contains(&chunk.1)
        })
    }
}
