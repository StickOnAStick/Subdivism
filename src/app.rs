use std::{
    collections::HashSet,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crossbeam_channel::{Receiver, Sender, TryRecvError, unbounded};
use glam::Vec3;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, WindowAttributes, WindowId},
};

use crate::{
    camera::{Camera, CameraLens},
    debug_overlay::{DebugOverlay, MenuLayout, OverlayVertex},
    game::{
        actor::{ActorRoster, PLAYER_EYE_HEIGHT},
        hud::HudFormatter,
        interact::{
            RaycastHit, SubTarget, direction_to_screen, face_plane_points, find_sub_block_at_point,
            push_screen_line, raycast_world_detailed, sub_block_face_outline_points,
            sub_slot_overlaps_existing, sub_target_from_world_point, voxel_coords, world_to_screen,
        },
        inventory::{BACKPACK_COLS, BACKPACK_ROWS, BACKPACK_SIZE, HOTBAR_SIZE, Inventory},
        physics::{self, MovementInput, PhysicsConfig},
        terrain_params::TerrainParamRegistry,
        terrain_recipe::TerrainRecipe,
        world::{
            Block, CHUNK_SIZE, DEFAULT_WORLD_SEED, TerrainConfig, WORLD_MAX_Y, WORLD_MIN_Y,
            WORLD_OVERWORLD_FLOOR, World,
        },
    },
    mesh::Vertex,
    render::{ChunkRenderKey, GpuState, GraphicsSettings, RenderOutcome, celestial_state_for_time},
};

#[path = "app/components.rs"]
mod components;
#[path = "app/settings.rs"]
mod settings;
use components::{
    BuildModeState, CameraRuntimeState, ChunkStreamingState, DiagnosticsState, MenuState,
    PlatformRuntimeState, RuntimeState, WorldSessionState,
};
use settings::AppSettings;

const FREE_CAMERA_SPEED: f32 = 10.0;
const LOOK_SENSITIVITY: f32 = 0.0025;
const MAX_PITCH: f32 = 1.54;
const FRAME_CAP_PRESETS: [Option<u32>; 5] = [None, Some(60), Some(120), Some(144), Some(240)];
const DEFAULT_FRAME_CAP_INDEX: usize = 2;
const DEFAULT_RENDER_DISTANCE_CHUNKS: u32 = 24;
const MIN_RENDER_DISTANCE_CHUNKS: u32 = 2;
const MAX_RENDER_DISTANCE_CHUNKS: u32 = 128;
const MIN_LDO_START_DISTANCE_CHUNKS: u32 = 6;
const MAX_LDO_START_DISTANCE_CHUNKS: u32 = 48;
const LOW_PRESET_RENDER_DISTANCE_CHUNKS: u32 = 12;
const MIN_CHUNK_UPLOADS_PER_FRAME: usize = 8;
const MIN_CHUNK_REQUESTS_IN_FLIGHT: usize = 192;
const FULL_DETAIL_RADIUS_CHUNKS: i64 = 16;
const MID_DETAIL_RADIUS_CHUNKS: i64 = 32;
const LOW_DETAIL_RADIUS_CHUNKS: i64 = 64;
const DEFAULT_TERRAIN_RECIPE_PATH: &str = "terrain/default.terrain";
const TERRAIN_PRESET_RECIPE_DIR: &str = "terrain/presets";
const MAX_TERRAIN_LAB_SAVE_NAME_CHARS: usize = 40;

pub fn run() {
    let options = AppLaunchOptions::from_env();
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let mut app = App::new(options);
    event_loop.run_app(&mut app).expect("event loop error");
}

pub fn run_terrain_lab() {
    let mut options = AppLaunchOptions::from_env();
    options.terrain_lab = true;
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let mut app = App::new(options);
    event_loop.run_app(&mut app).expect("event loop error");
}

#[derive(Clone, Debug)]
struct AppLaunchOptions {
    seed_override: Option<i64>,
    terrain_file: Option<PathBuf>,
    dev_mode: bool,
    terrain_lab: bool,
}

impl AppLaunchOptions {
    fn from_env() -> Self {
        let mut seed_override = None;
        let mut terrain_file = None;
        let mut dev_mode = false;
        let mut terrain_lab = std::env::var("SUBDIVISM_TERRAIN_LAB")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
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
                "--terrain-lab" => {
                    terrain_lab = true;
                }
                _ => {}
            }
        }
        if terrain_file.is_none() {
            let default_path = PathBuf::from(DEFAULT_TERRAIN_RECIPE_PATH);
            if default_path.is_file() {
                terrain_file = Some(default_path);
            }
        }
        Self {
            seed_override,
            terrain_file,
            dev_mode,
            terrain_lab,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum SubdivideScale {
    Full,
    Thirds,
    Sixths,
}

impl SubdivideScale {
    fn label(self) -> &'static str {
        match self {
            Self::Full => "1X1",
            Self::Thirds => "3X3",
            Self::Sixths => "6X6",
        }
    }

    fn cycle(self, delta: i32) -> Self {
        let mut idx = match self {
            Self::Full => 0_i32,
            Self::Thirds => 1,
            Self::Sixths => 2,
        };
        idx = (idx + delta).rem_euclid(3);
        match idx {
            1 => Self::Thirds,
            2 => Self::Sixths,
            _ => Self::Full,
        }
    }
}

#[derive(Clone, Copy)]
struct InventoryCursor {
    section: InventorySection,
    index: usize,
}

struct InputState {
    pressed: HashSet<KeyCode>,
    mouse_captured: bool,
    cursor_position: Option<(f32, f32)>,
}

impl InputState {
    fn new() -> Self {
        Self {
            pressed: HashSet::new(),
            mouse_captured: false,
            cursor_position: None,
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
    request_tx: Sender<ChunkBuildRequest>,
    result_rx: Receiver<ChunkBuildResult>,
}

impl ChunkBuildPipeline {
    fn new(world: World) -> Self {
        let worker_count = desired_chunk_worker_count();
        let (request_tx, request_rx) = unbounded::<ChunkBuildRequest>();
        let (result_tx, result_rx) = unbounded::<ChunkBuildResult>();

        for worker_index in 0..worker_count {
            let rx = request_rx.clone();
            let tx = result_tx.clone();
            let worker_world = world.clone();
            let thread_name = format!("chunk-mesh-worker-{worker_index}");
            std::thread::Builder::new()
                .name(thread_name)
                .spawn(move || {
                    loop {
                        let request = match rx.recv() {
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

fn desired_chunk_worker_count() -> usize {
    if let Some(override_count) = chunk_worker_override_from_env() {
        return override_count;
    }

    let available = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);

    if available <= 2 {
        available.max(1)
    } else {
        available.saturating_sub(1).clamp(2, 8)
    }
}

fn chunk_worker_override_from_env() -> Option<usize> {
    std::env::var("SUBDIVISM_CHUNK_WORKERS")
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .map(|parsed| parsed.clamp(1, 12))
}

struct App {
    platform: PlatformRuntimeState,
    world: World,
    actors: ActorRoster,
    camera: CameraRuntimeState,
    menu: MenuState,
    inventory_cursor: InventoryCursor,
    input: InputState,
    physics: PhysicsConfig,
    chunk_stream: ChunkStreamingState,
    inventory: Inventory,
    session: WorldSessionState,
    runtime: RuntimeState,
    graphics_settings: GraphicsSettings,
    settings_path: PathBuf,
    diagnostics: DiagnosticsState,
    build_mode: BuildModeState,
}

impl App {
    fn new(options: AppLaunchOptions) -> Self {
        let settings_path = settings::default_settings_path();
        let persisted_settings = match AppSettings::load_from_file(&settings_path) {
            Ok(Some(settings)) => settings,
            Ok(None) => AppSettings::default(),
            Err(err) => {
                eprintln!(
                    "failed to load app settings {}: {err}",
                    settings_path.display()
                );
                AppSettings::default()
            }
        };

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
        if options.terrain_lab {
            terrain = TerrainConfig::zeroed();
            terrain_profile = "terrain_lab_zeroed".to_string();
            terrain_recipe_path = None;
        }

        let world = World::generate_with_terrain_and_seed(terrain, world_seed);
        let lens = CameraLens::default();
        let actors = ActorRoster::new(world.spawn_point());
        let free_camera = actors.local_player().camera(lens);
        let camera_mode = if options.terrain_lab {
            CameraMode::Free
        } else {
            CameraMode::Player
        };
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15);
        let mut chunk_stream = ChunkStreamingState::new(world.clone());
        chunk_stream.render_distance_chunks = persisted_settings.render_distance_chunks;
        let chunk_worker_count = desired_chunk_worker_count();
        let chunk_worker_env_override = chunk_worker_override_from_env();
        let mut debug_overlay = DebugOverlay::new();
        if options.terrain_lab {
            debug_overlay.visible = true;
        }
        let session = WorldSessionState::new(
            world_seed,
            terrain_profile,
            terrain_recipe_path,
            options.dev_mode,
            options.terrain_lab,
            default_terrain_lab_save_name(world_seed),
        );
        let mut runtime = RuntimeState::new(seed);
        runtime.frame_cap_index = persisted_settings.frame_cap_index;
        let diagnostics =
            DiagnosticsState::new(debug_overlay, chunk_worker_count, chunk_worker_env_override);

        Self {
            platform: PlatformRuntimeState::new(),
            world,
            actors,
            camera: CameraRuntimeState::new(camera_mode, free_camera, lens),
            menu: MenuState::new(),
            inventory_cursor: InventoryCursor {
                section: InventorySection::Hotbar,
                index: 0,
            },
            input: InputState::new(),
            physics: PhysicsConfig::default(),
            chunk_stream,
            inventory: Inventory::new(),
            session,
            runtime,
            graphics_settings: persisted_settings.graphics_settings,
            settings_path,
            diagnostics,
            build_mode: BuildModeState::new(),
        }
    }

    fn active_camera(&self) -> Camera {
        match self.camera.mode {
            CameraMode::Player => self.actors.local_player().camera(self.camera.lens),
            CameraMode::Free => self.camera.free_camera,
        }
    }

    fn sync_active_camera(&mut self) {
        let camera = self.active_camera();
        if let Some(gpu) = self.platform.gpu.as_mut() {
            gpu.set_camera(camera);
        }
    }

    fn sync_lens_to_cameras(&mut self) {
        self.camera.free_camera.lens = self.camera.lens;
        self.sync_active_camera();
    }

    fn update_far_plane_for_render_distance(&mut self) {
        let base_far =
            ((self.chunk_stream.render_distance_chunks as f32 + 2.0) * CHUNK_SIZE as f32 * 2.0)
                .clamp(500.0, 8192.0);
        let camera = self.active_camera();
        let altitude_above_surface = (camera.position.y - WORLD_OVERWORLD_FLOOR as f32).max(0.0);
        let altitude_bonus = altitude_above_surface * 1.35;
        let target_far = (base_far + altitude_bonus).clamp(500.0, 8192.0);
        self.camera.lens.z_far = target_far;
        self.sync_lens_to_cameras();
    }

    fn replace_world(
        &mut self,
        world: World,
        world_seed: i64,
        terrain_profile: String,
        terrain_recipe_path: Option<PathBuf>,
    ) {
        self.replace_world_internal(
            world,
            world_seed,
            terrain_profile,
            terrain_recipe_path,
            false,
        );
    }

    fn replace_world_internal(
        &mut self,
        world: World,
        world_seed: i64,
        terrain_profile: String,
        terrain_recipe_path: Option<PathBuf>,
        preserve_view: bool,
    ) {
        let saved_actor = if preserve_view {
            Some(*self.actors.local_player())
        } else {
            None
        };
        let saved_free_camera = if preserve_view {
            Some(self.camera.free_camera)
        } else {
            None
        };
        let saved_camera_mode = self.camera.mode;

        let mut existing_keys = self.chunk_stream.visible_chunks.clone();
        existing_keys.extend(self.chunk_stream.resident_chunks.iter().copied());
        if let Some(gpu) = self.platform.gpu.as_mut() {
            for key in existing_keys {
                gpu.remove_chunk_mesh(key);
            }
        }

        self.world = world;
        self.session.world_seed = world_seed;
        self.session.terrain_profile = terrain_profile;
        self.session.terrain_recipe_path = terrain_recipe_path;
        self.chunk_stream.pipeline = ChunkBuildPipeline::new(self.world.clone());
        self.chunk_stream.visible_chunks.clear();
        self.chunk_stream.resident_chunks.clear();
        self.chunk_stream.requested_chunks.clear();
        self.chunk_stream.dirty_chunks.clear();
        self.chunk_stream.chunk_versions.clear();

        if preserve_view {
            if let Some(actor) = saved_actor {
                *self.actors.local_player_mut() = actor;
            }
            if let Some(camera) = saved_free_camera {
                self.camera.free_camera = camera;
            }
            self.camera.mode = saved_camera_mode;
        } else {
            self.actors.respawn_local_player(self.world.spawn_point());
            self.camera.free_camera = self.actors.local_player().camera(self.camera.lens);
        }
        self.chunk_stream.last_chunk_center = self.current_chunk_center();
        self.schedule_visible_chunks();
        self.sync_active_camera();
        self.refresh_window_title();
    }

    fn rebuild_world_from_terrain(&mut self, terrain: TerrainConfig, preserve_view: bool) {
        let world = World::generate_with_terrain_and_seed(terrain, self.session.world_seed);
        let profile = if self.session.terrain_lab.enabled {
            "terrain_lab".to_string()
        } else {
            self.session.terrain_profile.clone()
        };
        let path = if self.session.terrain_lab.enabled {
            self.session.terrain_recipe_path.clone()
        } else {
            self.session.terrain_recipe_path.clone()
        };
        self.replace_world_internal(world, self.session.world_seed, profile, path, preserve_view);
    }

    fn reload_dev_world(&mut self) {
        if !self.session.dev_mode {
            return;
        }
        let mut terrain = self.world.terrain_config();
        let mut seed = self.session.world_seed;
        let mut profile = self.session.terrain_profile.clone();
        let recipe_path = self.session.terrain_recipe_path.clone();

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
        FRAME_CAP_PRESETS[self.runtime.frame_cap_index]
    }

    fn frame_cap_label(&self) -> String {
        match self.current_frame_cap() {
            Some(fps) => format!("{fps} FPS cap"),
            None => "uncapped".to_string(),
        }
    }

    fn current_app_settings(&self) -> AppSettings {
        AppSettings::new(
            self.chunk_stream.render_distance_chunks,
            self.runtime.frame_cap_index,
            self.graphics_settings,
        )
    }

    fn persist_app_settings(&self) {
        let settings = self.current_app_settings();
        if let Err(err) = settings.write_to_file(&self.settings_path) {
            eprintln!(
                "failed to save app settings {}: {err}",
                self.settings_path.display()
            );
        }
    }

    fn refresh_window_title(&self) {
        if let Some(window) = &self.platform.window {
            let dev_flag = if self.session.dev_mode { " DEV" } else { "" };
            let lab_flag = if self.session.terrain_lab.enabled {
                " LAB"
            } else {
                ""
            };
            window.set_title(&format!(
                "Voxel Starter [{} | {} | RD {} | SEED {}{}{}]",
                self.frame_cap_label(),
                self.camera.mode.label(),
                self.chunk_stream.render_distance_chunks,
                self.session.world_seed,
                dev_flag,
                lab_flag
            ));
        }
    }

    fn cycle_frame_cap(&mut self) {
        self.runtime.frame_cap_index = (self.runtime.frame_cap_index + 1) % FRAME_CAP_PRESETS.len();
        self.runtime.next_frame_at = Instant::now();
        self.refresh_window_title();
        self.persist_app_settings();
    }

    fn toggle_camera_mode(&mut self) {
        self.camera.mode = match self.camera.mode {
            CameraMode::Player => {
                self.camera.free_camera = self.actors.local_player().camera(self.camera.lens);
                CameraMode::Free
            }
            CameraMode::Free => CameraMode::Player,
        };
        self.sync_active_camera();
        self.refresh_window_title();
    }

    fn capture_mouse(&mut self) {
        if let Some(window) = &self.platform.window {
            let _ = window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
            window.set_cursor_visible(false);
            self.input.mouse_captured = true;
        }
    }

    fn release_mouse(&mut self) {
        if let Some(window) = &self.platform.window {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
            self.input.mouse_captured = false;
        }
    }

    fn open_pause_menu(&mut self) {
        self.menu.ui_mode = UiMode::Paused;
        self.menu.page = MenuPage::Main;
        self.menu.index = 0;
        self.menu.hover_index = None;
        self.release_mouse();
    }

    fn close_pause_menu(&mut self) {
        self.menu.ui_mode = UiMode::Playing;
    }

    fn open_inventory(&mut self) {
        self.menu.ui_mode = UiMode::Inventory;
        self.inventory_cursor = InventoryCursor {
            section: InventorySection::Hotbar,
            index: self.inventory.selected_hotbar_index(),
        };
        self.release_mouse();
    }

    fn close_inventory(&mut self) {
        self.menu.ui_mode = UiMode::Playing;
    }

    fn update(&mut self, dt: f32) {
        if self.menu.ui_mode != UiMode::Playing {
            return;
        }
        match self.camera.mode {
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
        let forward = self.camera.free_camera.forward();
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
            self.camera.free_camera.position += move_dir.normalize() * FREE_CAMERA_SPEED * dt;
        }

        if self.world.is_out_of_bounds(
            self.camera.free_camera.position - Vec3::Y * PLAYER_EYE_HEIGHT,
            self.physics.respawn_margin,
        ) {
            self.camera.free_camera.position =
                self.world.spawn_point() + Vec3::Y * PLAYER_EYE_HEIGHT;
        }
    }

    fn current_chunk_center(&self) -> (i64, i64) {
        let cam = self.active_camera();
        let wx = cam.position.x.floor() as i64;
        let wz = cam.position.z.floor() as i64;
        World::world_to_chunk(wx, wz)
    }

    fn lod_ring_limits(&self) -> (i64, i64, i64) {
        let scale = self.graphics_settings.ldo_detail_scale.clamp(0.55, 1.8);
        let full = self
            .graphics_settings
            .ldo_start_distance_chunks
            .clamp(MIN_LDO_START_DISTANCE_CHUNKS, MAX_LDO_START_DISTANCE_CHUNKS)
            as i64;
        let mid_step = (((MID_DETAIL_RADIUS_CHUNKS - FULL_DETAIL_RADIUS_CHUNKS) as f32) * scale)
            .round() as i64;
        let low_step =
            (((LOW_DETAIL_RADIUS_CHUNKS - MID_DETAIL_RADIUS_CHUNKS) as f32) * scale).round() as i64;
        let mid = (full + mid_step.max(2)).max(full + 2);
        let low = (mid + low_step.max(2)).max(mid + 2);
        (full, mid, low)
    }

    fn max_chunk_requests_in_flight(&self) -> usize {
        let rd = self.chunk_stream.render_distance_chunks as usize;
        let target = 128 + rd.saturating_mul(10);
        let boosted = if self.session.terrain_lab.enabled {
            target.saturating_mul(2)
        } else {
            target
        };
        boosted.clamp(MIN_CHUNK_REQUESTS_IN_FLIGHT, 2048)
    }

    fn max_chunk_uploads_per_frame(&self) -> usize {
        let rd = self.chunk_stream.render_distance_chunks as usize;
        let base = if self.session.terrain_lab.enabled {
            20
        } else {
            MIN_CHUNK_UPLOADS_PER_FRAME
        };
        let scaled = base + rd / 8;
        scaled.clamp(base, 64)
    }

    fn has_pending_visible_chunk_work(&self) -> bool {
        if self.chunk_stream.visible_chunks.is_empty() {
            return false;
        }
        let loaded_or_inflight =
            self.chunk_stream.resident_chunks.len() + self.chunk_stream.requested_chunks.len();
        if loaded_or_inflight < self.chunk_stream.visible_chunks.len() {
            return true;
        }
        self.chunk_stream.dirty_chunks.iter().any(|key| {
            self.chunk_stream.visible_chunks.contains(key)
                && !self.chunk_stream.requested_chunks.contains(key)
        })
    }

    fn schedule_visible_chunks(&mut self) {
        let center = self.current_chunk_center();
        let render_distance = (self.chunk_stream.render_distance_chunks as i64)
            .clamp(1, MAX_RENDER_DISTANCE_CHUNKS as i64);
        let (full_detail_radius, mid_detail_radius, low_detail_radius) = self.lod_ring_limits();
        let mut desired = Vec::new();
        collect_lod_ring(
            &mut desired,
            center,
            0,
            render_distance.min(full_detail_radius),
            0,
        );
        if render_distance > full_detail_radius {
            collect_lod_ring(
                &mut desired,
                center,
                full_detail_radius,
                render_distance.min(mid_detail_radius),
                1,
            );
        }
        if render_distance > mid_detail_radius {
            collect_lod_ring(
                &mut desired,
                center,
                mid_detail_radius,
                render_distance.min(low_detail_radius),
                2,
            );
        }
        if render_distance > low_detail_radius {
            collect_lod_ring(&mut desired, center, low_detail_radius, render_distance, 3);
        }

        desired.sort_by_key(|key| {
            let dx = key.origin_chunk.0 - center.0;
            let dz = key.origin_chunk.1 - center.1;
            (dx * dx + dz * dz, key.lod_level)
        });
        let desired_set: HashSet<ChunkRenderKey> = desired.iter().copied().collect();

        self.chunk_stream.visible_chunks = desired_set;
        self.chunk_stream.last_chunk_center = center;
        let max_in_flight = self.max_chunk_requests_in_flight();

        for key in desired {
            if self.chunk_stream.resident_chunks.contains(&key)
                && !self.chunk_stream.dirty_chunks.contains(&key)
            {
                continue;
            }
            if self.chunk_stream.requested_chunks.len() >= max_in_flight {
                break;
            }
            self.ensure_chunk_requested(key);
        }
    }

    fn process_chunk_build_results(&mut self) {
        let mut uploads_remaining = self.max_chunk_uploads_per_frame();

        while uploads_remaining > 0 {
            let result = match self.chunk_stream.pipeline.result_rx.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            };

            self.chunk_stream.requested_chunks.remove(&result.key);
            let newest_version = self
                .chunk_stream
                .chunk_versions
                .get(&result.key)
                .copied()
                .unwrap_or(0);
            if result.version != newest_version {
                self.ensure_chunk_requested(result.key);
                continue;
            }
            if !self.chunk_stream.visible_chunks.contains(&result.key) {
                continue;
            }

            if let Some(gpu) = self.platform.gpu.as_mut() {
                gpu.upsert_chunk_mesh(result.key, &result.vertices);
            }
            self.chunk_stream.resident_chunks.insert(result.key);
            self.chunk_stream.dirty_chunks.remove(&result.key);
            uploads_remaining -= 1;
        }
    }

    fn ensure_chunk_requested(&mut self, key: ChunkRenderKey) {
        if self.chunk_stream.requested_chunks.contains(&key) {
            return;
        }
        let version = *self.chunk_stream.chunk_versions.entry(key).or_insert(1);
        if self
            .chunk_stream
            .pipeline
            .request_tx
            .send(ChunkBuildRequest { key, version })
            .is_ok()
        {
            self.chunk_stream.requested_chunks.insert(key);
        }
    }

    fn mark_chunk_dirty(&mut self, key: ChunkRenderKey) {
        let version = self
            .chunk_stream
            .chunk_versions
            .entry(key)
            .and_modify(|v| *v = v.saturating_add(1))
            .or_insert(1);
        self.chunk_stream.dirty_chunks.insert(key);
        if self.chunk_stream.visible_chunks.contains(&key)
            && !self.chunk_stream.requested_chunks.contains(&key)
        {
            let _ = self
                .chunk_stream
                .pipeline
                .request_tx
                .send(ChunkBuildRequest {
                    key,
                    version: *version,
                });
            self.chunk_stream.requested_chunks.insert(key);
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
            let x = random_i32_inclusive(&mut self.runtime.rng_state, min_x, max_x);
            let z = random_i32_inclusive(&mut self.runtime.rng_state, min_z, max_z);
            if let Some(candidate) = self.world.spawn_point_for_column(x, z) {
                spawn = candidate;
                break;
            }
        }

        self.actors.respawn_local_player(spawn);
        self.camera.free_camera = self.actors.local_player().camera(self.camera.lens);
        self.sync_active_camera();
    }

    fn handle_menu_navigation(&mut self, code: KeyCode) {
        let item_count = self.menu_item_count();
        match code {
            KeyCode::ArrowUp => {
                self.menu.index = if self.menu.index == 0 {
                    item_count - 1
                } else {
                    self.menu.index - 1
                };
                self.menu.hover_index = None;
            }
            KeyCode::ArrowDown => {
                self.menu.index = (self.menu.index + 1) % item_count;
                self.menu.hover_index = None;
            }
            KeyCode::ArrowLeft => {
                self.adjust_menu_value(-1);
            }
            KeyCode::ArrowRight => {
                self.adjust_menu_value(1);
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
            KeyCode::ArrowUp => match self.inventory_cursor.section {
                InventorySection::Hotbar => {}
                InventorySection::Backpack => {
                    let row = self.inventory_cursor.index / BACKPACK_COLS;
                    let col = self.inventory_cursor.index % BACKPACK_COLS;
                    if row == 0 {
                        self.inventory_cursor.section = InventorySection::Hotbar;
                        self.inventory_cursor.index = col.min(HOTBAR_SIZE - 1);
                    } else {
                        self.inventory_cursor.index -= BACKPACK_COLS;
                    }
                }
            },
            KeyCode::ArrowDown => match self.inventory_cursor.section {
                InventorySection::Hotbar => {
                    self.inventory_cursor.section = InventorySection::Backpack;
                    self.inventory_cursor.index =
                        self.inventory_cursor.index.min(BACKPACK_COLS - 1);
                }
                InventorySection::Backpack => {
                    let row = self.inventory_cursor.index / BACKPACK_COLS;
                    if row + 1 < BACKPACK_ROWS {
                        self.inventory_cursor.index += BACKPACK_COLS;
                    }
                }
            },
            KeyCode::ArrowLeft => match self.inventory_cursor.section {
                InventorySection::Hotbar => {
                    self.inventory_cursor.index = if self.inventory_cursor.index == 0 {
                        HOTBAR_SIZE - 1
                    } else {
                        self.inventory_cursor.index - 1
                    };
                }
                InventorySection::Backpack => {
                    let row = self.inventory_cursor.index / BACKPACK_COLS;
                    let col = self.inventory_cursor.index % BACKPACK_COLS;
                    let next_col = if col == 0 { BACKPACK_COLS - 1 } else { col - 1 };
                    self.inventory_cursor.index = row * BACKPACK_COLS + next_col;
                }
            },
            KeyCode::ArrowRight => match self.inventory_cursor.section {
                InventorySection::Hotbar => {
                    self.inventory_cursor.index = (self.inventory_cursor.index + 1) % HOTBAR_SIZE;
                }
                InventorySection::Backpack => {
                    let row = self.inventory_cursor.index / BACKPACK_COLS;
                    let col = self.inventory_cursor.index % BACKPACK_COLS;
                    let next_col = (col + 1) % BACKPACK_COLS;
                    self.inventory_cursor.index = row * BACKPACK_COLS + next_col;
                }
            },
            KeyCode::Enter | KeyCode::Space => {
                self.activate_inventory_selection();
            }
            KeyCode::Escape | KeyCode::KeyE => {
                self.close_inventory();
            }
            _ => {}
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
        self.menu_overlay().1.len()
    }

    fn activate_menu_item(&mut self) {
        match self.menu.page {
            MenuPage::Main => match self.menu.index {
                0 => self.close_pause_menu(),
                1 => {
                    self.respawn_player_random_near_center();
                    self.close_pause_menu();
                }
                2 => {
                    self.menu.page = MenuPage::Settings;
                    self.menu.index = 0;
                }
                3 => {
                    self.runtime.should_exit = true;
                }
                _ => {}
            },
            MenuPage::Settings => match self.menu.index {
                0 => {
                    self.menu.page = MenuPage::Graphics;
                    self.menu.index = 0;
                }
                1 => {
                    self.menu.page = MenuPage::Main;
                    self.menu.index = 0;
                }
                _ => {}
            },
            MenuPage::Graphics => match self.menu.index {
                0 => self.adjust_render_distance(1),
                1 => self.adjust_graphics_setting_by(1, 1.0),
                2 => self.adjust_graphics_setting_by(2, 0.05),
                3 => {
                    self.graphics_settings.shadows_enabled =
                        !self.graphics_settings.shadows_enabled;
                    self.persist_app_settings();
                }
                4 => {
                    self.graphics_settings.fog_enabled = !self.graphics_settings.fog_enabled;
                    self.persist_app_settings();
                }
                5 => {
                    self.graphics_settings.atmosphere_enabled =
                        !self.graphics_settings.atmosphere_enabled;
                    self.persist_app_settings();
                }
                6 => self.adjust_graphics_setting_by(6, 0.05),
                7 => self.adjust_graphics_setting_by(7, 0.03),
                8 => self.adjust_graphics_setting_by(8, 0.05),
                9 => self.adjust_graphics_setting_by(9, 0.05),
                10 => self.adjust_graphics_setting_by(10, 0.02),
                11 => self.adjust_graphics_setting_by(11, 0.04),
                12 => self.adjust_graphics_setting_by(12, 8.0),
                13 => self.adjust_graphics_setting_by(13, 16.0),
                14 => self.adjust_graphics_setting_by(14, 0.03),
                15 => self.adjust_graphics_setting_by(15, 0.03),
                16 => {
                    self.apply_low_graphics_preset();
                }
                17 => {
                    self.graphics_settings = GraphicsSettings::default();
                    self.persist_app_settings();
                }
                18 => {
                    self.menu.page = MenuPage::Settings;
                    self.menu.index = 0;
                }
                _ => {}
            },
        }
        let item_count = self.menu_item_count().max(1);
        self.menu.index = self.menu.index.min(item_count - 1);
        self.menu.hover_index = None;
    }

    fn menu_back_or_resume(&mut self) {
        match self.menu.page {
            MenuPage::Main => self.close_pause_menu(),
            MenuPage::Settings => {
                self.menu.page = MenuPage::Main;
                self.menu.index = 0;
            }
            MenuPage::Graphics => {
                self.menu.page = MenuPage::Settings;
                self.menu.index = 0;
            }
        }
        self.menu.hover_index = None;
    }

    fn adjust_render_distance(&mut self, delta: i32) {
        let previous = self.chunk_stream.render_distance_chunks;
        let value = self.chunk_stream.render_distance_chunks as i32 + delta;
        let next = value.clamp(
            MIN_RENDER_DISTANCE_CHUNKS as i32,
            MAX_RENDER_DISTANCE_CHUNKS as i32,
        ) as u32;
        if next == previous {
            return;
        }
        self.chunk_stream.render_distance_chunks = next;
        self.update_far_plane_for_render_distance();
        self.schedule_visible_chunks();
        self.refresh_window_title();
        self.persist_app_settings();
    }

    fn apply_low_graphics_preset(&mut self) {
        self.graphics_settings = GraphicsSettings::low_preset();
        self.chunk_stream.render_distance_chunks = LOW_PRESET_RENDER_DISTANCE_CHUNKS
            .clamp(MIN_RENDER_DISTANCE_CHUNKS, MAX_RENDER_DISTANCE_CHUNKS);
        self.update_far_plane_for_render_distance();
        self.schedule_visible_chunks();
        self.refresh_window_title();
        self.persist_app_settings();
    }

    fn menu_overlay(&self) -> (String, Vec<String>, usize) {
        match self.menu.page {
            MenuPage::Main => (
                "PAUSED".to_string(),
                vec![
                    "RESUME".to_string(),
                    "RESPAWN".to_string(),
                    "SETTINGS".to_string(),
                    "QUIT TO DESKTOP".to_string(),
                ],
                self.menu.index,
            ),
            MenuPage::Settings => (
                "SETTINGS".to_string(),
                vec!["GRAPHICS".to_string(), "BACK".to_string()],
                self.menu.index,
            ),
            MenuPage::Graphics => (
                "GRAPHICS".to_string(),
                vec![
                    format!(
                        "RENDER DISTANCE {}",
                        slider_u32(
                            self.chunk_stream.render_distance_chunks,
                            MIN_RENDER_DISTANCE_CHUNKS,
                            MAX_RENDER_DISTANCE_CHUNKS,
                            14
                        )
                    ),
                    format!(
                        "LDO START {}",
                        slider_u32(
                            self.graphics_settings.ldo_start_distance_chunks,
                            MIN_LDO_START_DISTANCE_CHUNKS,
                            MAX_LDO_START_DISTANCE_CHUNKS,
                            14
                        )
                    ),
                    format!(
                        "LDO DETAIL {}",
                        slider_f32(self.graphics_settings.ldo_detail_scale, 0.55, 1.8, 14)
                    ),
                    format!(
                        "SHADOWS {}",
                        if self.graphics_settings.shadows_enabled {
                            "ON"
                        } else {
                            "OFF"
                        }
                    ),
                    format!(
                        "FOG {}",
                        if self.graphics_settings.fog_enabled {
                            "ON"
                        } else {
                            "OFF"
                        }
                    ),
                    format!(
                        "ATMOSPHERE FX {}",
                        if self.graphics_settings.atmosphere_enabled {
                            "ON"
                        } else {
                            "OFF"
                        }
                    ),
                    format!(
                        "SHADER QUALITY {}",
                        slider_f32(self.graphics_settings.shader_quality, 0.0, 1.0, 14)
                    ),
                    format!(
                        "AMBIENT BOOST {}",
                        slider_f32(self.graphics_settings.ambient_boost, 0.0, 0.70, 14)
                    ),
                    format!(
                        "SHADOW SOFTNESS {}",
                        slider_f32(self.graphics_settings.shadow_softness, 0.1, 1.5, 14)
                    ),
                    format!(
                        "SHADOW CONTRAST {}",
                        slider_f32(self.graphics_settings.shadow_contrast, 0.2, 2.2, 14)
                    ),
                    format!(
                        "FAR SHADOW LIFT {}",
                        slider_f32(self.graphics_settings.far_shadow_lift, 0.0, 0.6, 14)
                    ),
                    format!(
                        "FOG STRENGTH {}",
                        slider_f32(self.graphics_settings.fog_strength, 0.0, 1.2, 14)
                    ),
                    format!(
                        "FOG START {}",
                        slider_f32(self.graphics_settings.fog_start, 16.0, 1200.0, 14)
                    ),
                    format!(
                        "FOG END {}",
                        slider_f32(self.graphics_settings.fog_end, 24.0, 2000.0, 14)
                    ),
                    format!(
                        "ATMOSPHERE {}",
                        slider_f32(self.graphics_settings.atmosphere_strength, 0.0, 1.0, 14)
                    ),
                    format!(
                        "VIBRANCE {}",
                        slider_f32(self.graphics_settings.color_vibrance, 0.5, 1.8, 14)
                    ),
                    "APPLY LOW PRESET (RD 12)".to_string(),
                    "RESET TO DEFAULTS".to_string(),
                    "BACK".to_string(),
                ],
                self.menu.index,
            ),
        }
    }

    fn adjust_menu_value(&mut self, delta: i32) {
        if self.menu.page != MenuPage::Graphics {
            return;
        }
        match self.menu.index {
            0 => self.adjust_render_distance(delta),
            1 => self.adjust_graphics_setting_by(1, delta as f32),
            2 => self.adjust_graphics_setting_by(2, 0.05 * delta as f32),
            3 => {
                let next = delta > 0;
                if self.graphics_settings.shadows_enabled != next {
                    self.graphics_settings.shadows_enabled = next;
                    self.persist_app_settings();
                }
            }
            4 => {
                let next = delta > 0;
                if self.graphics_settings.fog_enabled != next {
                    self.graphics_settings.fog_enabled = next;
                    self.persist_app_settings();
                }
            }
            5 => {
                let next = delta > 0;
                if self.graphics_settings.atmosphere_enabled != next {
                    self.graphics_settings.atmosphere_enabled = next;
                    self.persist_app_settings();
                }
            }
            6 => self.adjust_graphics_setting_by(6, 0.05 * delta as f32),
            7 => self.adjust_graphics_setting_by(7, 0.03 * delta as f32),
            8 => self.adjust_graphics_setting_by(8, 0.05 * delta as f32),
            9 => self.adjust_graphics_setting_by(9, 0.05 * delta as f32),
            10 => self.adjust_graphics_setting_by(10, 0.02 * delta as f32),
            11 => self.adjust_graphics_setting_by(11, 0.04 * delta as f32),
            12 => self.adjust_graphics_setting_by(12, 8.0 * delta as f32),
            13 => self.adjust_graphics_setting_by(13, 16.0 * delta as f32),
            14 => self.adjust_graphics_setting_by(14, 0.03 * delta as f32),
            15 => self.adjust_graphics_setting_by(15, 0.03 * delta as f32),
            _ => {}
        }
    }

    fn adjust_graphics_setting_by(&mut self, index: usize, delta: f32) {
        let mut changed = false;
        match index {
            1 => {
                let value = self.graphics_settings.ldo_start_distance_chunks as i32 + delta as i32;
                let next = value.clamp(
                    MIN_LDO_START_DISTANCE_CHUNKS as i32,
                    MAX_LDO_START_DISTANCE_CHUNKS as i32,
                ) as u32;
                if next != self.graphics_settings.ldo_start_distance_chunks {
                    self.graphics_settings.ldo_start_distance_chunks = next;
                    self.schedule_visible_chunks();
                    changed = true;
                }
            }
            2 => {
                let next = (self.graphics_settings.ldo_detail_scale + delta).clamp(0.55, 1.8);
                if (next - self.graphics_settings.ldo_detail_scale).abs() > f32::EPSILON {
                    self.graphics_settings.ldo_detail_scale = next;
                    self.schedule_visible_chunks();
                    changed = true;
                }
            }
            6 => {
                let next = (self.graphics_settings.shader_quality + delta).clamp(0.0, 1.0);
                if (next - self.graphics_settings.shader_quality).abs() > f32::EPSILON {
                    self.graphics_settings.shader_quality = next;
                    changed = true;
                }
            }
            7 => {
                let next = (self.graphics_settings.ambient_boost + delta).clamp(0.0, 0.70);
                if (next - self.graphics_settings.ambient_boost).abs() > f32::EPSILON {
                    self.graphics_settings.ambient_boost = next;
                    changed = true;
                }
            }
            8 => {
                let next = (self.graphics_settings.shadow_softness + delta).clamp(0.1, 1.5);
                if (next - self.graphics_settings.shadow_softness).abs() > f32::EPSILON {
                    self.graphics_settings.shadow_softness = next;
                    changed = true;
                }
            }
            9 => {
                let next = (self.graphics_settings.shadow_contrast + delta).clamp(0.2, 2.2);
                if (next - self.graphics_settings.shadow_contrast).abs() > f32::EPSILON {
                    self.graphics_settings.shadow_contrast = next;
                    changed = true;
                }
            }
            10 => {
                let next = (self.graphics_settings.far_shadow_lift + delta).clamp(0.0, 0.6);
                if (next - self.graphics_settings.far_shadow_lift).abs() > f32::EPSILON {
                    self.graphics_settings.far_shadow_lift = next;
                    changed = true;
                }
            }
            11 => {
                let next = (self.graphics_settings.fog_strength + delta).clamp(0.0, 1.2);
                if (next - self.graphics_settings.fog_strength).abs() > f32::EPSILON {
                    self.graphics_settings.fog_strength = next;
                    changed = true;
                }
            }
            12 => {
                let previous_start = self.graphics_settings.fog_start;
                let previous_end = self.graphics_settings.fog_end;
                let next_start = (previous_start + delta).clamp(16.0, 1200.0);
                let mut next_end = previous_end;
                if next_end <= next_start + 1.0 {
                    next_end = next_start + 1.0;
                }
                if (next_start - previous_start).abs() > f32::EPSILON
                    || (next_end - previous_end).abs() > f32::EPSILON
                {
                    self.graphics_settings.fog_start = next_start;
                    self.graphics_settings.fog_end = next_end;
                    changed = true;
                }
            }
            13 => {
                let previous_start = self.graphics_settings.fog_start;
                let previous_end = self.graphics_settings.fog_end;
                let next_end = (previous_end + delta).clamp(24.0, 2000.0);
                let mut next_start = previous_start;
                if next_end <= next_start + 1.0 {
                    next_start = (next_end - 1.0).max(16.0);
                }
                if (next_start - previous_start).abs() > f32::EPSILON
                    || (next_end - previous_end).abs() > f32::EPSILON
                {
                    self.graphics_settings.fog_start = next_start;
                    self.graphics_settings.fog_end = next_end;
                    changed = true;
                }
            }
            14 => {
                let next = (self.graphics_settings.atmosphere_strength + delta).clamp(0.0, 1.0);
                if (next - self.graphics_settings.atmosphere_strength).abs() > f32::EPSILON {
                    self.graphics_settings.atmosphere_strength = next;
                    changed = true;
                }
            }
            15 => {
                let next = (self.graphics_settings.color_vibrance + delta).clamp(0.5, 1.8);
                if (next - self.graphics_settings.color_vibrance).abs() > f32::EPSILON {
                    self.graphics_settings.color_vibrance = next;
                    changed = true;
                }
            }
            _ => {}
        }
        if changed {
            self.persist_app_settings();
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
        let mut lines = Vec::with_capacity(16);
        lines.push("ARROWS MOVE ENTER APPLY E CLOSE".to_string());
        lines.push("HOTBAR 1X8".to_string());
        lines.push(HudFormatter::format_inventory_row(
            self.inventory.hotbar(),
            0,
            HOTBAR_SIZE,
        ));
        lines.push(format!(
            "ACTIVE {} {} {}",
            selected_hotbar + 1,
            HudFormatter::block_label(hotbar_slot.block),
            hotbar_slot.count
        ));
        lines.push("BACKPACK 6X8".to_string());
        for row in 0..BACKPACK_ROWS {
            let start = row * BACKPACK_COLS;
            lines.push(HudFormatter::format_inventory_row(
                self.inventory.backpack(),
                start,
                BACKPACK_COLS,
            ));
        }
        lines.push(format!(
            "CURSOR {} {} {}",
            if matches!(self.inventory_cursor.section, InventorySection::Hotbar) {
                "HOTBAR"
            } else {
                "BACKPACK"
            },
            backpack_index + 1,
            HudFormatter::block_label(backpack_slot.block)
        ));

        let selected_line = match self.inventory_cursor.section {
            InventorySection::Hotbar => 2,
            InventorySection::Backpack => 5 + self.inventory_cursor.index / BACKPACK_COLS,
        };

        ("INVENTORY".to_string(), lines, selected_line)
    }

    fn terrain_lab_overlay(&self) -> Option<(String, Vec<String>, usize)> {
        if !self.session.terrain_lab.enabled || !self.session.terrain_lab.panel_visible {
            return None;
        }
        let keys = TerrainConfig::parameter_keys();
        let max_rows = 12_usize;
        let selected = self
            .session
            .terrain_lab
            .selected_index
            .min(keys.len().saturating_sub(1));
        let page_start = selected.saturating_sub(max_rows / 2);
        let page_end = (page_start + max_rows).min(keys.len());
        let shown = &keys[page_start..page_end];
        let mut lines = Vec::with_capacity(shown.len() + 8);
        lines.push("WHEEL/UPDOWN SELECT LEFTRIGHT TUNE".to_string());
        lines.push("SHIFT FINE CTRL ULTRA-FINE PGUP/PGDN COARSE".to_string());
        lines.push("F8 PANEL F9 SAVE PRESET F10 EDIT NAME".to_string());
        if self.session.terrain_lab.editing_save_name {
            lines.push("NAME EDIT ACTIVE TYPE / BKSP ENTER=SAVE ESC=CANCEL".to_string());
        } else {
            lines.push("NAME EDIT OFF".to_string());
        }
        lines.push(format!("SAVE NAME {}", self.session.terrain_lab.save_name));
        if !self.session.terrain_lab.last_save_status.is_empty() {
            lines.push(self.session.terrain_lab.last_save_status.clone());
        }
        let header_rows = lines.len();
        let cfg = self.world.terrain_config();
        for key in shown {
            let value = TerrainParamRegistry::value(&cfg, key);
            lines.push(format!(
                "{} {}",
                TerrainParamRegistry::label(key),
                slider_f32(
                    value,
                    TerrainParamRegistry::min(key),
                    TerrainParamRegistry::max(key),
                    12
                )
            ));
        }
        Some((
            "TERRAIN LAB".to_string(),
            lines,
            (selected - page_start) + header_rows,
        ))
    }

    fn terrain_lab_step_for(&self, key: &str) -> f32 {
        let base = TerrainParamRegistry::step(key);
        let ctrl_held =
            self.input.key(KeyCode::ControlLeft) || self.input.key(KeyCode::ControlRight);
        let shift_held = self.input.key(KeyCode::ShiftLeft) || self.input.key(KeyCode::ShiftRight);
        if ctrl_held {
            base * 0.1
        } else if shift_held {
            base * 0.25
        } else {
            base
        }
    }

    fn terrain_lab_adjust_selected(&mut self, delta: f32) {
        if !self.session.terrain_lab.enabled {
            return;
        }
        let keys = TerrainConfig::parameter_keys();
        if keys.is_empty() {
            return;
        }
        let index = self.session.terrain_lab.selected_index.min(keys.len() - 1);
        let key = keys[index];
        let mut cfg = self.world.terrain_config();
        let current = TerrainParamRegistry::value(&cfg, key);
        if cfg.set_named_param(key, current + delta).is_err() {
            return;
        }
        self.rebuild_world_from_terrain(cfg, true);
    }

    fn terrain_lab_save_preset(&mut self) {
        if !self.session.terrain_lab.enabled {
            return;
        }
        let typed_name = self.session.terrain_lab.save_name.trim();
        let terrain_name = if typed_name.is_empty() {
            default_terrain_lab_save_name(self.session.world_seed)
        } else {
            typed_name.to_string()
        };
        self.session.terrain_lab.save_name = terrain_name.clone();
        let file_stem = sanitize_terrain_recipe_name(&terrain_name);
        let named_path =
            PathBuf::from(TERRAIN_PRESET_RECIPE_DIR).join(format!("{file_stem}.terrain"));
        let default_path = PathBuf::from(DEFAULT_TERRAIN_RECIPE_PATH);

        let mut recipe = TerrainRecipe::balanced(self.session.world_seed);
        recipe.name = terrain_name;
        recipe.description = "Saved from in-game Terrain Lab".to_string();
        recipe.profile = "terrain_lab".to_string();
        recipe.seed = self.session.world_seed;
        recipe.terrain = self.world.terrain_config();
        if let Err(err) = recipe.write_to_file(&named_path) {
            self.session.terrain_lab.last_save_status = format!("SAVE FAILED {err}");
            eprintln!(
                "failed to save named terrain preset {}: {err}",
                named_path.display()
            );
            return;
        }
        if let Err(err) = recipe.write_to_file(&default_path) {
            self.session.terrain_lab.last_save_status = format!("DEFAULT IMPORT FAILED {err}");
            eprintln!(
                "failed to update default terrain recipe {}: {err}",
                default_path.display()
            );
            return;
        }
        self.session.terrain_profile = "terrain_lab".to_string();
        self.session.terrain_recipe_path = Some(default_path.clone());
        self.session.terrain_lab.last_save_status = format!(
            "SAVED {} (DEFAULT IMPORT {})",
            named_path.display(),
            default_path.display()
        );
        eprintln!(
            "terrain lab preset saved to {} and imported as {}",
            named_path.display(),
            default_path.display()
        );
    }

    fn handle_terrain_lab_name_input(&mut self, code: KeyCode) -> bool {
        if !self.session.terrain_lab.editing_save_name {
            return false;
        }
        match code {
            KeyCode::Escape => {
                self.session.terrain_lab.editing_save_name = false;
                return true;
            }
            KeyCode::Enter => {
                self.terrain_lab_save_preset();
                self.session.terrain_lab.editing_save_name = false;
                return true;
            }
            KeyCode::Backspace => {
                self.session.terrain_lab.save_name.pop();
                return true;
            }
            _ => {}
        }
        let shift_held = self.input.key(KeyCode::ShiftLeft) || self.input.key(KeyCode::ShiftRight);
        if let Some(ch) = terrain_lab_save_name_char(code, shift_held)
            && self.session.terrain_lab.save_name.len() < MAX_TERRAIN_LAB_SAVE_NAME_CHARS
        {
            self.session.terrain_lab.save_name.push(ch);
            return true;
        }
        true
    }

    fn handle_terrain_lab_input(&mut self, code: KeyCode, repeat: bool) -> bool {
        if !self.session.terrain_lab.enabled {
            return false;
        }
        if code == KeyCode::F8 && !repeat {
            self.session.terrain_lab.panel_visible = !self.session.terrain_lab.panel_visible;
            return true;
        }
        if code == KeyCode::F9 && !repeat {
            self.terrain_lab_save_preset();
            return true;
        }
        if !self.session.terrain_lab.panel_visible {
            return false;
        }
        if code == KeyCode::F10 && !repeat {
            self.session.terrain_lab.editing_save_name =
                !self.session.terrain_lab.editing_save_name;
            if self.session.terrain_lab.editing_save_name
                && self.session.terrain_lab.save_name.trim().is_empty()
            {
                self.session.terrain_lab.save_name =
                    default_terrain_lab_save_name(self.session.world_seed);
            }
            return true;
        }
        if self.session.terrain_lab.editing_save_name {
            return self.handle_terrain_lab_name_input(code);
        }
        let keys = TerrainConfig::parameter_keys();
        if keys.is_empty() {
            return false;
        }
        match code {
            KeyCode::ArrowUp if !repeat => {
                self.session.terrain_lab.selected_index =
                    self.session.terrain_lab.selected_index.saturating_sub(1);
                true
            }
            KeyCode::ArrowDown if !repeat => {
                self.session.terrain_lab.selected_index =
                    (self.session.terrain_lab.selected_index + 1).min(keys.len() - 1);
                true
            }
            KeyCode::ArrowLeft => {
                let index = self.session.terrain_lab.selected_index.min(keys.len() - 1);
                let step = self.terrain_lab_step_for(keys[index]);
                self.terrain_lab_adjust_selected(-step);
                true
            }
            KeyCode::ArrowRight => {
                let index = self.session.terrain_lab.selected_index.min(keys.len() - 1);
                let step = self.terrain_lab_step_for(keys[index]);
                self.terrain_lab_adjust_selected(step);
                true
            }
            KeyCode::PageDown => {
                let index = self.session.terrain_lab.selected_index.min(keys.len() - 1);
                let step = self.terrain_lab_step_for(keys[index]);
                self.terrain_lab_adjust_selected(-step * 5.0);
                true
            }
            KeyCode::PageUp => {
                let index = self.session.terrain_lab.selected_index.min(keys.len() - 1);
                let step = self.terrain_lab_step_for(keys[index]);
                self.terrain_lab_adjust_selected(step * 5.0);
                true
            }
            _ => false,
        }
    }

    fn current_menu_layout(&self) -> Option<MenuLayout> {
        if self.menu.ui_mode != UiMode::Paused {
            return None;
        }
        let window = self.platform.window.as_ref()?;
        let size = window.inner_size();
        let screen_size = [size.width as f32, size.height as f32];
        let (title, lines, _) = self.menu_overlay();
        Some(
            self.diagnostics
                .debug_overlay
                .menu_layout(&title, &lines, screen_size),
        )
    }

    fn update_menu_hover_from_cursor(&mut self) {
        if self.menu.ui_mode != UiMode::Paused {
            self.menu.hover_index = None;
            return;
        }
        let Some((cursor_x, cursor_y)) = self.input.cursor_position else {
            self.menu.hover_index = None;
            return;
        };
        let Some(layout) = self.current_menu_layout() else {
            self.menu.hover_index = None;
            return;
        };
        self.menu.hover_index = layout
            .item_rects
            .iter()
            .position(|rect| rect.contains(cursor_x, cursor_y));
    }

    fn handle_menu_wheel(&mut self, delta_y: f32) {
        if self.menu.ui_mode != UiMode::Paused || delta_y.abs() <= f32::EPSILON {
            return;
        }
        let direction = if delta_y > 0.0 { 1 } else { -1 };
        self.update_menu_hover_from_cursor();
        if let Some(index) = self.menu.hover_index {
            self.menu.index = index;
            if self.menu.page == MenuPage::Graphics && self.menu.index <= 15 {
                self.adjust_menu_value(direction);
                return;
            }
        }
        let count = self.menu_item_count().max(1);
        self.menu.index = if direction > 0 {
            self.menu.index.saturating_sub(1)
        } else {
            (self.menu.index + 1) % count
        };
    }

    fn handle_terrain_lab_wheel(&mut self, delta_y: f32) -> bool {
        if !self.session.terrain_lab.enabled
            || !self.session.terrain_lab.panel_visible
            || self.menu.ui_mode != UiMode::Playing
            || delta_y.abs() <= f32::EPSILON
        {
            return false;
        }
        let keys = TerrainConfig::parameter_keys();
        if keys.is_empty() {
            return false;
        }
        let direction = if delta_y > 0.0 { -1 } else { 1 };
        self.session.terrain_lab.selected_index = if direction < 0 {
            self.session.terrain_lab.selected_index.saturating_sub(1)
        } else {
            (self.session.terrain_lab.selected_index + 1).min(keys.len() - 1)
        };
        true
    }

    fn hud_lines(&self) -> Vec<String> {
        let camera = self.active_camera();
        let (wx, wy, wz) = (
            camera.position.x.floor() as i64,
            camera.position.y.floor() as i32,
            camera.position.z.floor() as i64,
        );
        let (cx, cz) = World::world_to_chunk(wx, wz);
        let slot = self.inventory.selected_slot();
        let hotbar = self.inventory.hotbar();
        let mut lines = vec![
            format!("SUN {:.1}", self.runtime.world_time_seconds),
            format!("SEED {}", self.session.world_seed),
            format!(
                "XYZ {:.1} {:.1} {:.1}",
                camera.position.x, camera.position.y, camera.position.z
            ),
            format!("BLOCK {} {} {}", wx, wy, wz),
            format!("CHUNK {} {}", cx, cz),
            format!(
                "SLOT {} {} {}",
                self.inventory.selected_hotbar_index() + 1,
                HudFormatter::block_label(slot.block),
                slot.count
            ),
            format!(
                "HOTBAR {} {} {} {}",
                hotbar.first().map(|slot| slot.count).unwrap_or(0),
                hotbar.get(1).map(|slot| slot.count).unwrap_or(0),
                hotbar.get(2).map(|slot| slot.count).unwrap_or(0),
                hotbar.get(3).map(|slot| slot.count).unwrap_or(0)
            ),
            format!(
                "INV {} OF {}",
                self.inventory.filled_slots(),
                self.inventory.slot_capacity()
            ),
            format!("BUILD SCALE {}", self.build_mode.subdivide_scale.label()),
            format!(
                "LDO START {} DETAIL {:.2}",
                self.graphics_settings.ldo_start_distance_chunks,
                self.graphics_settings.ldo_detail_scale
            ),
            match self.diagnostics.chunk_worker_env_override {
                Some(override_count) => format!(
                    "CHUNK WORKERS {} (ENV {})",
                    self.diagnostics.chunk_worker_count, override_count
                ),
                None => format!(
                    "CHUNK WORKERS {} (AUTO)",
                    self.diagnostics.chunk_worker_count
                ),
            },
        ];
        if self.session.dev_mode {
            lines.push(format!(
                "DEV PROFILE {}",
                self.session.terrain_profile.to_uppercase()
            ));
            if self.session.terrain_recipe_path.is_some() {
                lines.push("DEV FILE ON".to_string());
                lines.push("F6 RELOAD".to_string());
            }
        }
        if self.session.terrain_lab.enabled {
            lines.push("LAB F8 PANEL F9 SAVE".to_string());
        }
        lines
    }

    fn sky_body_overlay(&self) -> Vec<OverlayVertex> {
        let mut vertices = Vec::new();
        let Some(window) = &self.platform.window else {
            return vertices;
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return vertices;
        }
        let camera = self.active_camera();
        let celestial = celestial_state_for_time(self.runtime.world_time_seconds);
        let sky_body_occluded = |direction: Vec3| -> bool {
            raycast_world_detailed(&self.world, camera.position, direction, 4096.0, 0.75, None)
                .is_some()
        };

        if celestial.sun_direction.y > -0.03
            && !sky_body_occluded(celestial.sun_direction)
            && let Some((x, y)) = direction_to_screen(
                camera,
                celestial.sun_direction,
                size.width as f32,
                size.height as f32,
            )
        {
            DebugOverlay::add_quad(
                &mut vertices,
                x - 64.0,
                y - 64.0,
                128.0,
                128.0,
                [1.0, 0.66, 0.28, 0.10],
            );
            push_block_sprite(
                &mut vertices,
                x - 48.0,
                y - 48.0,
                96.0,
                &SUN_BLOCK_PALETTE,
                &SUN_BLOCK_PATTERN,
            );
        }
        if celestial.moon_direction.y > -0.03
            && !sky_body_occluded(celestial.moon_direction)
            && let Some((x, y)) = direction_to_screen(
                camera,
                celestial.moon_direction,
                size.width as f32,
                size.height as f32,
            )
        {
            DebugOverlay::add_quad(
                &mut vertices,
                x - 56.0,
                y - 56.0,
                112.0,
                112.0,
                [0.64, 0.72, 0.92, 0.08],
            );
            push_block_sprite(
                &mut vertices,
                x - 42.0,
                y - 42.0,
                84.0,
                &MOON_BLOCK_PALETTE,
                &MOON_BLOCK_PATTERN,
            );
        }

        vertices
    }

    fn gameplay_overlay_vertices(&self, screen_size: [f32; 2]) -> Vec<OverlayVertex> {
        let mut vertices = Vec::new();
        self.push_hotbar_overlay(&mut vertices, screen_size);
        self.push_held_item_overlay(&mut vertices, screen_size);
        if self.build_mode.subdivide_scale != SubdivideScale::Full && self.input.mouse_captured {
            self.push_subdivision_overlay(&mut vertices, screen_size);
        }
        vertices
    }

    fn push_hotbar_overlay(&self, vertices: &mut Vec<OverlayVertex>, screen_size: [f32; 2]) {
        let slot_w = 56.0_f32;
        let slot_h = 42.0_f32;
        let gap = 8.0_f32;
        let total_w = HOTBAR_SIZE as f32 * slot_w + (HOTBAR_SIZE.saturating_sub(1)) as f32 * gap;
        let start_x = (screen_size[0] - total_w) * 0.5;
        let y = screen_size[1] - slot_h - 18.0;
        let hotbar = self.inventory.hotbar();

        for (i, slot) in hotbar.iter().enumerate() {
            let x = start_x + i as f32 * (slot_w + gap);
            let selected = i == self.inventory.selected_hotbar_index();
            let bg = if selected {
                [0.18, 0.30, 0.28, 0.88]
            } else {
                [0.05, 0.07, 0.08, 0.76]
            };
            let border = if selected {
                [0.82, 0.95, 0.90, 0.90]
            } else {
                [0.24, 0.30, 0.33, 0.72]
            };
            DebugOverlay::add_quad(vertices, x, y, slot_w, slot_h, border);
            DebugOverlay::add_quad(vertices, x + 2.0, y + 2.0, slot_w - 4.0, slot_h - 4.0, bg);

            let swatch = HudFormatter::block_tint_color(slot.block);
            DebugOverlay::add_quad(vertices, x + 8.0, y + 9.0, 16.0, 16.0, swatch);
            let label = if slot.is_empty() {
                "___".to_string()
            } else {
                format!(
                    "{}{:02}",
                    HudFormatter::block_short_code(slot.block),
                    slot.count.min(99)
                )
            };
            DebugOverlay::add_text(
                vertices,
                x + 28.0,
                y + 12.0,
                &label,
                2.0,
                [0.90, 0.95, 0.93, 1.0],
            );
        }
    }

    fn push_held_item_overlay(&self, vertices: &mut Vec<OverlayVertex>, screen_size: [f32; 2]) {
        let slot = self.inventory.selected_slot();
        if slot.is_empty() {
            return;
        }
        let x = screen_size[0] - 66.0;
        let y = screen_size[1] - 82.0;
        let tint = HudFormatter::block_tint_color(slot.block);
        DebugOverlay::add_quad(vertices, x, y, 22.0, 22.0, [0.02, 0.03, 0.04, 0.75]);
        DebugOverlay::add_quad(vertices, x + 3.0, y + 3.0, 16.0, 16.0, tint);
    }

    fn push_subdivision_overlay(&self, vertices: &mut Vec<OverlayVertex>, screen_size: [f32; 2]) {
        let camera = self.active_camera();
        let divisions = self.current_divisions();
        let Some(hit) = raycast_world_detailed(
            &self.world,
            camera.position,
            camera.forward(),
            7.0,
            0.03,
            Some(divisions),
        ) else {
            return;
        };

        if let Some((sub, _)) = find_sub_block_at_point(&self.world, hit.cell, hit.point) {
            let edge_color = [0.10, 0.13, 0.16, 0.94];
            for (a, b) in sub_block_face_outline_points(sub, hit.normal) {
                if let (Some(pa), Some(pb)) = (
                    world_to_screen(camera, a, screen_size[0], screen_size[1]),
                    world_to_screen(camera, b, screen_size[0], screen_size[1]),
                ) {
                    push_screen_line(vertices, pa, pb, 1.8, edge_color);
                }
            }
            return;
        }

        let plane = face_plane_points(hit.cell, hit.normal, divisions);
        let color = [0.14, 0.18, 0.20, 0.80];
        for (a, b) in plane {
            if let (Some(pa), Some(pb)) = (
                world_to_screen(camera, a, screen_size[0], screen_size[1]),
                world_to_screen(camera, b, screen_size[0], screen_size[1]),
            ) {
                push_screen_line(vertices, pa, pb, 1.5, color);
            }
        }
    }

    fn edit_block_from_click(&mut self, remove: bool) {
        if self.menu.ui_mode != UiMode::Playing {
            return;
        }
        let camera = self.active_camera();
        let Some(hit) = raycast_world_detailed(
            &self.world,
            camera.position,
            camera.forward(),
            7.0,
            0.03,
            if self.build_mode.subdivide_scale == SubdivideScale::Full {
                None
            } else {
                Some(self.current_divisions())
            },
        ) else {
            return;
        };

        if self.build_mode.subdivide_scale != SubdivideScale::Full {
            if self.edit_sub_block_from_click(remove, hit) {
                return;
            }
            // In subdivision mode, still allow normal block breaking when no
            // sub-block target exists.
            if !remove {
                return;
            }
        }

        if remove {
            let block = self.world.block_at_i64(hit.cell.0, hit.cell.1, hit.cell.2);
            if matches!(block, Block::Air) {
                return;
            }
            if !self.inventory.add_block(block) {
                return;
            }
            self.world
                .set_block_i64(hit.cell.0, hit.cell.1, hit.cell.2, Block::Air);
            self.mark_block_change_dirty(hit.cell.0, hit.cell.2);
            return;
        }

        let place = hit.previous;
        if place.1 <= WORLD_MIN_Y || place.1 >= WORLD_MAX_Y {
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

    fn cycle_subdivide_scale(&mut self, delta: i32) {
        self.build_mode.subdivide_scale = self.build_mode.subdivide_scale.cycle(delta);
    }

    fn current_divisions(&self) -> u8 {
        match self.build_mode.subdivide_scale {
            SubdivideScale::Full => 1,
            SubdivideScale::Thirds => 3,
            SubdivideScale::Sixths => 6,
        }
    }

    fn sub_piece_cost_units(&self) -> u16 {
        match self.build_mode.subdivide_scale {
            SubdivideScale::Thirds => 2,
            SubdivideScale::Sixths => 1,
            SubdivideScale::Full => 6,
        }
    }

    fn spend_sub_units(&mut self, block: Block, units: u16) -> bool {
        let wallet = self.build_mode.sub_unit_wallet.entry(block).or_insert(0);
        while *wallet < units {
            if !self.inventory.try_take_selected_matching(block) {
                return false;
            }
            *wallet += 6;
        }
        *wallet -= units;
        true
    }

    fn refund_sub_units(&mut self, block: Block, units: u16) {
        if !block.is_solid() {
            return;
        }
        let wallet = self.build_mode.sub_unit_wallet.entry(block).or_insert(0);
        *wallet += units;
        while *wallet >= 6 {
            if self.inventory.add_block(block) {
                *wallet -= 6;
            } else {
                break;
            }
        }
    }

    fn edit_sub_block_from_click(&mut self, remove: bool, hit: RaycastHit) -> bool {
        let divisions = self.current_divisions();
        if divisions <= 1 {
            return false;
        }
        if remove {
            let mut picked = find_sub_block_at_point(&self.world, hit.cell, hit.point);
            if picked.is_none() {
                let probe = hit.point - hit.normal.as_vec3() * 0.0025;
                let probe_cell = voxel_coords(probe);
                picked = find_sub_block_at_point(&self.world, probe_cell, probe);
            }
            if let Some((hit_sub, block)) = picked {
                let _ = self.world.clear_sub_block_i64(
                    hit_sub.x,
                    hit_sub.y,
                    hit_sub.z,
                    hit_sub.divisions,
                    hit_sub.sx,
                    hit_sub.sy,
                    hit_sub.sz,
                );
                self.refund_sub_units(block, self.sub_piece_cost_units());
                self.mark_block_change_dirty(hit_sub.x, hit_sub.z);
                return true;
            }
            return false;
        }

        let Some(slot_block) = self
            .inventory
            .hotbar()
            .get(self.inventory.selected_hotbar_index())
            .copied()
            .map(|slot| slot.block)
        else {
            return false;
        };
        if !slot_block.is_solid() {
            return false;
        }

        let target =
            if let Some((hit_sub, _)) = find_sub_block_at_point(&self.world, hit.cell, hit.point) {
                let mut base = (hit_sub.x, hit_sub.y, hit_sub.z);
                let mut sx = hit_sub.sx as i32 + hit.normal.x;
                let mut sy = hit_sub.sy as i32 + hit.normal.y;
                let mut sz = hit_sub.sz as i32 + hit.normal.z;
                let d = divisions as i32;

                if sx < 0 {
                    base.0 -= 1;
                    sx = d - 1;
                } else if sx >= d {
                    base.0 += 1;
                    sx = 0;
                }
                if sy < 0 {
                    base.1 -= 1;
                    sy = d - 1;
                } else if sy >= d {
                    base.1 += 1;
                    sy = 0;
                }
                if sz < 0 {
                    base.2 -= 1;
                    sz = d - 1;
                } else if sz >= d {
                    base.2 += 1;
                    sz = 0;
                }

                if base.1 < WORLD_MIN_Y || base.1 > WORLD_MAX_Y {
                    return false;
                }

                SubTarget {
                    base,
                    sx: sx as u8,
                    sy: sy as u8,
                    sz: sz as u8,
                }
            } else {
                let place_point = if hit.previous != hit.cell {
                    // Crossing into a solid base block: place into the air cell we came from.
                    hit.previous_point
                } else {
                    // Sub-cell hit inside same base cell: step outward along hit normal.
                    hit.point + hit.normal.as_vec3() * 0.0035
                };
                let target_base = voxel_coords(place_point);
                sub_target_from_world_point(target_base, place_point, divisions)
            };
        if self
            .world
            .block_at_i64(target.base.0, target.base.1, target.base.2)
            .is_solid()
        {
            return false;
        }
        if sub_slot_overlaps_existing(
            &self.world,
            target.base,
            divisions,
            target.sx,
            target.sy,
            target.sz,
        ) {
            return false;
        }

        let unit_cost = self.sub_piece_cost_units();
        if !self.world.set_sub_block_i64(
            target.base.0,
            target.base.1,
            target.base.2,
            divisions,
            target.sx,
            target.sy,
            target.sz,
            slot_block,
        ) {
            return false;
        }
        if !self.spend_sub_units(slot_block, unit_cost) {
            let _ = self.world.clear_sub_block_i64(
                target.base.0,
                target.base.1,
                target.base.2,
                divisions,
                target.sx,
                target.sy,
                target.sz,
            );
            return false;
        }
        self.mark_block_change_dirty(target.base.0, target.base.2);
        true
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.platform.window.is_some() {
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
        self.camera.lens = self
            .camera
            .lens
            .with_aspect(size.width.max(1) as f32 / size.height.max(1) as f32);
        self.update_far_plane_for_render_distance();
        self.camera.free_camera = self.actors.local_player().camera(self.camera.lens);
        let gpu = pollster::block_on(GpuState::new(window.clone(), self.active_camera()));

        self.platform.window_id = Some(window.id());
        self.platform.window = Some(window);
        self.platform.gpu = Some(gpu);
        self.runtime.last_frame = Instant::now();
        self.runtime.next_frame_at = self.runtime.last_frame;
        self.chunk_stream.last_chunk_center = self.current_chunk_center();
        self.schedule_visible_chunks();
        self.refresh_window_title();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if Some(window_id) != self.platform.window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.platform.gpu.as_mut() {
                    gpu.resize(size);
                }
                self.camera.lens = self
                    .camera
                    .lens
                    .with_aspect(size.width.max(1) as f32 / size.height.max(1) as f32);
                self.sync_lens_to_cameras();
            }
            WindowEvent::RedrawRequested => {
                let screen_size = if let Some(window) = &self.platform.window {
                    let size = window.inner_size();
                    [size.width as f32, size.height as f32]
                } else {
                    [1280.0, 720.0]
                };
                let menu_overlay = match self.menu.ui_mode {
                    UiMode::Paused => {
                        let (title, lines, selected) = self.menu_overlay();
                        Some(self.diagnostics.debug_overlay.build_menu_vertices(
                            &title,
                            &lines,
                            selected,
                            self.menu.hover_index,
                            screen_size,
                        ))
                    }
                    UiMode::Inventory => {
                        let (title, lines, selected) = self.inventory_overlay();
                        Some(self.diagnostics.debug_overlay.build_menu_vertices(
                            &title,
                            &lines,
                            selected,
                            None,
                            screen_size,
                        ))
                    }
                    UiMode::Playing => None,
                };
                let terrain_lab_overlay = if self.menu.ui_mode == UiMode::Playing {
                    if let Some((title, lines, selected)) = self.terrain_lab_overlay() {
                        Some(self.diagnostics.debug_overlay.build_menu_vertices(
                            &title,
                            &lines,
                            selected,
                            None,
                            screen_size,
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                };
                let gameplay_overlay = if self.menu.ui_mode == UiMode::Playing {
                    self.gameplay_overlay_vertices(screen_size)
                } else {
                    Vec::new()
                };
                let hud_lines = self.hud_lines();
                let sky_overlay = self.sky_body_overlay();
                if let Some(gpu) = self.platform.gpu.as_mut() {
                    let mut overlay_vertices = sky_overlay;
                    overlay_vertices.extend(gameplay_overlay);
                    overlay_vertices.extend(
                        self.diagnostics
                            .debug_overlay
                            .build_vertices(gpu.estimated_gpu_memory_bytes(), &hud_lines),
                    );
                    if let Some(menu_vertices) = menu_overlay {
                        overlay_vertices.extend(menu_vertices);
                    }
                    if let Some(lab_vertices) = terrain_lab_overlay {
                        overlay_vertices.extend(lab_vertices);
                    }
                    match gpu.render(&overlay_vertices) {
                        RenderOutcome::Success | RenderOutcome::SkipFrame => {}
                        RenderOutcome::Reconfigure => {
                            if let Some(window) = &self.platform.window {
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

                            if self.menu.ui_mode == UiMode::Paused {
                                self.handle_menu_navigation(code);
                                return;
                            }
                            if self.menu.ui_mode == UiMode::Inventory {
                                self.handle_inventory_navigation(code);
                                return;
                            }
                            if self.handle_terrain_lab_input(code, event.repeat) {
                                return;
                            }

                            if code == KeyCode::F3 && !event.repeat {
                                self.diagnostics.debug_overlay.visible =
                                    !self.diagnostics.debug_overlay.visible;
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
            WindowEvent::CursorMoved { position, .. } => {
                self.input.cursor_position = Some((position.x as f32, position.y as f32));
                self.update_menu_hover_from_cursor();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let delta_y = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 40.0,
                };
                if self.handle_terrain_lab_wheel(delta_y) {
                    return;
                }
                if self.menu.ui_mode == UiMode::Playing
                    && self.input.mouse_captured
                    && raycast_world_detailed(
                        &self.world,
                        self.active_camera().position,
                        self.active_camera().forward(),
                        7.0,
                        0.03,
                        None,
                    )
                    .is_some()
                    && delta_y.abs() > f32::EPSILON
                {
                    self.cycle_subdivide_scale(if delta_y > 0.0 { 1 } else { -1 });
                    return;
                }
                self.handle_menu_wheel(delta_y);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => {
                if self.menu.ui_mode == UiMode::Paused
                    && (button == MouseButton::Left || button == MouseButton::Right)
                {
                    self.update_menu_hover_from_cursor();
                    if let Some(index) = self.menu.hover_index {
                        self.menu.index = index;
                        if button == MouseButton::Left {
                            self.activate_menu_item();
                        } else {
                            self.adjust_menu_value(-1);
                        }
                    }
                    return;
                }
                if self.menu.ui_mode != UiMode::Playing {
                    return;
                }
                if !self.input.mouse_captured && button == MouseButton::Left {
                    self.capture_mouse();
                    return;
                }
                if self.input.mouse_captured {
                    let ctrl_held = self.input.key(KeyCode::ControlLeft)
                        || self.input.key(KeyCode::ControlRight);
                    if ctrl_held {
                        match button {
                            MouseButton::Left => self.cycle_subdivide_scale(-1),
                            MouseButton::Right => self.cycle_subdivide_scale(1),
                            _ => {}
                        }
                        return;
                    }
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
        if !self.input.mouse_captured || self.menu.ui_mode != UiMode::Playing {
            return;
        }

        if let DeviceEvent::MouseMotion { delta } = event {
            match self.camera.mode {
                CameraMode::Player => {
                    let actor = self.actors.local_player_mut();
                    actor.look.yaw += delta.0 as f32 * LOOK_SENSITIVITY;
                    actor.look.pitch -= delta.1 as f32 * LOOK_SENSITIVITY;
                    actor.look.pitch = actor.look.pitch.clamp(-MAX_PITCH, MAX_PITCH);
                }
                CameraMode::Free => {
                    self.camera.free_camera.yaw += delta.0 as f32 * LOOK_SENSITIVITY;
                    self.camera.free_camera.pitch -= delta.1 as f32 * LOOK_SENSITIVITY;
                    self.camera.free_camera.pitch =
                        self.camera.free_camera.pitch.clamp(-MAX_PITCH, MAX_PITCH);
                }
            }
            self.sync_active_camera();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.runtime.should_exit {
            event_loop.exit();
            return;
        }
        let now = Instant::now();
        if let Some(target_fps) = self.current_frame_cap() {
            let frame_interval = Duration::from_secs_f64(1.0 / target_fps as f64);
            if now < self.runtime.next_frame_at {
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.runtime.next_frame_at));
                return;
            }
            self.runtime.next_frame_at = now + frame_interval;
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.runtime.next_frame_at));
        } else {
            self.runtime.next_frame_at = now;
            event_loop.set_control_flow(ControlFlow::Poll);
        }

        let dt = (now - self.runtime.last_frame).as_secs_f32();
        self.runtime.last_frame = now;
        self.runtime.world_time_seconds += dt;
        self.diagnostics.debug_overlay.record_frame(dt);
        self.update(dt);
        let center = self.current_chunk_center();
        if center != self.chunk_stream.last_chunk_center {
            self.schedule_visible_chunks();
        }
        self.process_chunk_build_results();
        if center == self.chunk_stream.last_chunk_center
            && self.chunk_stream.requested_chunks.len() < self.max_chunk_requests_in_flight()
            && self.has_pending_visible_chunk_work()
        {
            self.schedule_visible_chunks();
        }
        let camera = self.active_camera();
        if let Some(gpu) = self.platform.gpu.as_mut() {
            gpu.set_environment(
                self.runtime.world_time_seconds,
                camera.position,
                self.world.terrain_seed(),
                self.graphics_settings,
            );
        }

        if let Some(window) = &self.platform.window {
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
        _ => None,
    }
}

fn default_terrain_lab_save_name(seed: i64) -> String {
    format!("terrain_lab_seed_{seed}")
}

fn sanitize_terrain_recipe_name(raw: &str) -> String {
    let lowered = raw.trim().to_ascii_lowercase();
    let mapped = lowered
        .chars()
        .map(|ch| match ch {
            'a'..='z' | '0'..='9' => ch,
            '-' | '_' => ch,
            ' ' => '_',
            _ => '_',
        })
        .collect::<String>();
    let collapsed = mapped
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if collapsed.is_empty() {
        "terrain".to_string()
    } else {
        collapsed
    }
}

fn terrain_lab_save_name_char(code: KeyCode, shift_held: bool) -> Option<char> {
    match code {
        KeyCode::KeyA => Some(if shift_held { 'A' } else { 'a' }),
        KeyCode::KeyB => Some(if shift_held { 'B' } else { 'b' }),
        KeyCode::KeyC => Some(if shift_held { 'C' } else { 'c' }),
        KeyCode::KeyD => Some(if shift_held { 'D' } else { 'd' }),
        KeyCode::KeyE => Some(if shift_held { 'E' } else { 'e' }),
        KeyCode::KeyF => Some(if shift_held { 'F' } else { 'f' }),
        KeyCode::KeyG => Some(if shift_held { 'G' } else { 'g' }),
        KeyCode::KeyH => Some(if shift_held { 'H' } else { 'h' }),
        KeyCode::KeyI => Some(if shift_held { 'I' } else { 'i' }),
        KeyCode::KeyJ => Some(if shift_held { 'J' } else { 'j' }),
        KeyCode::KeyK => Some(if shift_held { 'K' } else { 'k' }),
        KeyCode::KeyL => Some(if shift_held { 'L' } else { 'l' }),
        KeyCode::KeyM => Some(if shift_held { 'M' } else { 'm' }),
        KeyCode::KeyN => Some(if shift_held { 'N' } else { 'n' }),
        KeyCode::KeyO => Some(if shift_held { 'O' } else { 'o' }),
        KeyCode::KeyP => Some(if shift_held { 'P' } else { 'p' }),
        KeyCode::KeyQ => Some(if shift_held { 'Q' } else { 'q' }),
        KeyCode::KeyR => Some(if shift_held { 'R' } else { 'r' }),
        KeyCode::KeyS => Some(if shift_held { 'S' } else { 's' }),
        KeyCode::KeyT => Some(if shift_held { 'T' } else { 't' }),
        KeyCode::KeyU => Some(if shift_held { 'U' } else { 'u' }),
        KeyCode::KeyV => Some(if shift_held { 'V' } else { 'v' }),
        KeyCode::KeyW => Some(if shift_held { 'W' } else { 'w' }),
        KeyCode::KeyX => Some(if shift_held { 'X' } else { 'x' }),
        KeyCode::KeyY => Some(if shift_held { 'Y' } else { 'y' }),
        KeyCode::KeyZ => Some(if shift_held { 'Z' } else { 'z' }),
        KeyCode::Digit0 => Some('0'),
        KeyCode::Digit1 => Some('1'),
        KeyCode::Digit2 => Some('2'),
        KeyCode::Digit3 => Some('3'),
        KeyCode::Digit4 => Some('4'),
        KeyCode::Digit5 => Some('5'),
        KeyCode::Digit6 => Some('6'),
        KeyCode::Digit7 => Some('7'),
        KeyCode::Digit8 => Some('8'),
        KeyCode::Digit9 => Some('9'),
        KeyCode::Minus => Some(if shift_held { '_' } else { '-' }),
        KeyCode::Space => Some(' '),
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

fn slider_f32(value: f32, min: f32, max: f32, width: usize) -> String {
    let norm = if (max - min).abs() <= f32::EPSILON {
        0.0
    } else {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    };
    let filled = (norm * width as f32).round() as usize;
    let bar = format!(
        "{}{}",
        "_".repeat(filled.min(width)),
        "-".repeat(width.saturating_sub(filled.min(width)))
    );
    format!("{} {:.3}", bar, value)
}

fn slider_u32(value: u32, min: u32, max: u32, width: usize) -> String {
    let norm = if max <= min {
        0.0
    } else {
        (value.saturating_sub(min)) as f32 / (max - min) as f32
    };
    let filled = (norm * width as f32).round() as usize;
    let bar = format!(
        "{}{}",
        "_".repeat(filled.min(width)),
        "-".repeat(width.saturating_sub(filled.min(width)))
    );
    format!("{} {}", bar, value)
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

const SKY_BLOCK_GRID: usize = 8;
const SUN_BLOCK_PALETTE: [[f32; 4]; 4] = [
    [0.00, 0.00, 0.00, 0.00],
    [0.95, 0.46, 0.14, 0.96],
    [1.00, 0.70, 0.30, 0.98],
    [1.00, 0.88, 0.55, 0.98],
];
const SUN_BLOCK_PATTERN: [u8; 64] = [
    0, 0, 1, 1, 1, 1, 0, 0, 0, 1, 2, 2, 2, 2, 1, 0, 1, 2, 3, 3, 3, 3, 2, 1, 1, 2, 3, 2, 2, 3, 2, 1,
    1, 2, 3, 2, 2, 3, 2, 1, 1, 2, 3, 3, 3, 3, 2, 1, 0, 1, 2, 2, 2, 2, 1, 0, 0, 0, 1, 1, 1, 1, 0, 0,
];
const MOON_BLOCK_PALETTE: [[f32; 4]; 4] = [
    [0.00, 0.00, 0.00, 0.00],
    [0.42, 0.47, 0.56, 0.96],
    [0.62, 0.69, 0.80, 0.98],
    [0.80, 0.86, 0.94, 0.98],
];
const MOON_BLOCK_PATTERN: [u8; 64] = [
    0, 0, 1, 1, 1, 1, 0, 0, 0, 1, 2, 2, 3, 2, 1, 0, 1, 2, 2, 3, 2, 2, 2, 1, 1, 2, 3, 2, 2, 3, 2, 1,
    1, 3, 2, 2, 3, 2, 2, 1, 1, 2, 2, 3, 2, 2, 2, 1, 0, 1, 2, 2, 2, 2, 1, 0, 0, 0, 1, 1, 1, 1, 0, 0,
];

fn push_block_sprite(
    vertices: &mut Vec<OverlayVertex>,
    x: f32,
    y: f32,
    size: f32,
    palette: &[[f32; 4]; 4],
    pattern: &[u8; 64],
) {
    if size <= 0.0 {
        return;
    }

    let cell = size / SKY_BLOCK_GRID as f32;
    for gy in 0..SKY_BLOCK_GRID {
        for gx in 0..SKY_BLOCK_GRID {
            let index = gy * SKY_BLOCK_GRID + gx;
            let tone = pattern[index] as usize;
            if tone == 0 {
                continue;
            }
            let color = palette[tone.min(palette.len() - 1)];
            if color[3] <= 0.0 {
                continue;
            }
            DebugOverlay::add_quad(
                vertices,
                x + gx as f32 * cell,
                y + gy as f32 * cell,
                cell + 0.25,
                cell + 0.25,
                color,
            );
        }
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
