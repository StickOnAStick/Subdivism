use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

use winit::window::{Window, WindowId};

use crate::{
    camera::{Camera, CameraLens},
    debug_overlay::DebugOverlay,
    game::world::Block,
    render::{ChunkRenderKey, GpuState},
};

use super::{
    CameraMode, ChunkBuildPipeline, DEFAULT_FRAME_CAP_INDEX, DEFAULT_RENDER_DISTANCE_CHUNKS,
    MenuPage, SubdivideScale, UiMode, World,
};

pub(super) struct MenuState {
    pub(super) ui_mode: UiMode,
    pub(super) page: MenuPage,
    pub(super) index: usize,
    pub(super) hover_index: Option<usize>,
}

impl MenuState {
    pub(super) fn new() -> Self {
        Self {
            ui_mode: UiMode::Playing,
            page: MenuPage::Main,
            index: 0,
            hover_index: None,
        }
    }
}

pub(super) struct TerrainLabState {
    pub(super) enabled: bool,
    pub(super) panel_visible: bool,
    pub(super) selected_index: usize,
    pub(super) save_name: String,
    pub(super) editing_save_name: bool,
    pub(super) last_save_status: String,
}

impl TerrainLabState {
    pub(super) fn new(enabled: bool, initial_save_name: String) -> Self {
        Self {
            enabled,
            panel_visible: false,
            selected_index: 0,
            save_name: initial_save_name,
            editing_save_name: false,
            last_save_status: String::new(),
        }
    }
}

pub(super) struct PlatformRuntimeState {
    pub(super) window: Option<Arc<Window>>,
    pub(super) window_id: Option<WindowId>,
    pub(super) gpu: Option<GpuState>,
}

impl PlatformRuntimeState {
    pub(super) fn new() -> Self {
        Self {
            window: None,
            window_id: None,
            gpu: None,
        }
    }
}

pub(super) struct CameraRuntimeState {
    pub(super) mode: CameraMode,
    pub(super) free_camera: Camera,
    pub(super) lens: CameraLens,
}

impl CameraRuntimeState {
    pub(super) fn new(mode: CameraMode, free_camera: Camera, lens: CameraLens) -> Self {
        Self {
            mode,
            free_camera,
            lens,
        }
    }
}

pub(super) struct WorldSessionState {
    pub(super) world_seed: i64,
    pub(super) terrain_profile: String,
    pub(super) terrain_recipe_path: Option<PathBuf>,
    pub(super) dev_mode: bool,
    pub(super) terrain_lab: TerrainLabState,
}

impl WorldSessionState {
    pub(super) fn new(
        world_seed: i64,
        terrain_profile: String,
        terrain_recipe_path: Option<PathBuf>,
        dev_mode: bool,
        terrain_lab_enabled: bool,
        terrain_lab_save_name: String,
    ) -> Self {
        Self {
            world_seed,
            terrain_profile,
            terrain_recipe_path,
            dev_mode,
            terrain_lab: TerrainLabState::new(terrain_lab_enabled, terrain_lab_save_name),
        }
    }
}

pub(super) struct RuntimeState {
    pub(super) frame_cap_index: usize,
    pub(super) world_time_seconds: f32,
    pub(super) last_frame: Instant,
    pub(super) next_frame_at: Instant,
    pub(super) rng_state: u64,
    pub(super) should_exit: bool,
}

impl RuntimeState {
    pub(super) fn new(seed: u64) -> Self {
        let now = Instant::now();
        Self {
            frame_cap_index: DEFAULT_FRAME_CAP_INDEX,
            world_time_seconds: 0.0,
            last_frame: now,
            next_frame_at: now,
            rng_state: seed,
            should_exit: false,
        }
    }
}

pub(super) struct BuildModeState {
    pub(super) subdivide_scale: SubdivideScale,
    pub(super) sub_unit_wallet: HashMap<Block, u16>,
}

impl BuildModeState {
    pub(super) fn new() -> Self {
        Self {
            subdivide_scale: SubdivideScale::Full,
            sub_unit_wallet: HashMap::new(),
        }
    }
}

pub(super) struct DiagnosticsState {
    pub(super) debug_overlay: DebugOverlay,
    pub(super) chunk_worker_count: usize,
    pub(super) chunk_worker_env_override: Option<usize>,
}

impl DiagnosticsState {
    pub(super) fn new(
        debug_overlay: DebugOverlay,
        chunk_worker_count: usize,
        chunk_worker_env_override: Option<usize>,
    ) -> Self {
        Self {
            debug_overlay,
            chunk_worker_count,
            chunk_worker_env_override,
        }
    }
}

pub(super) struct ChunkStreamingState {
    pub(super) render_distance_chunks: u32,
    pub(super) visible_chunks: HashSet<ChunkRenderKey>,
    pub(super) resident_chunks: HashSet<ChunkRenderKey>,
    pub(super) requested_chunks: HashSet<ChunkRenderKey>,
    pub(super) dirty_chunks: HashSet<ChunkRenderKey>,
    pub(super) chunk_versions: HashMap<ChunkRenderKey, u64>,
    pub(super) pipeline: ChunkBuildPipeline,
    pub(super) last_chunk_center: (i64, i64),
}

impl ChunkStreamingState {
    pub(super) fn new(world: World) -> Self {
        Self {
            render_distance_chunks: DEFAULT_RENDER_DISTANCE_CHUNKS,
            visible_chunks: HashSet::new(),
            resident_chunks: HashSet::new(),
            requested_chunks: HashSet::new(),
            dirty_chunks: HashSet::new(),
            chunk_versions: HashMap::new(),
            pipeline: ChunkBuildPipeline::new(world),
            last_chunk_center: (0, 0),
        }
    }
}
