use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

use glam::Vec3;
use winit::window::{Window, WindowId};

use crate::{
    camera::{Camera, CameraLens},
    debug_overlay::DebugOverlay,
    game::{
        actor::ActorState,
        physics::MovementInput,
        world::Block,
    },
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
    pub(super) chase_camera_position: Vec3,
    pub(super) chase_distance: f32,
    pub(super) chase_height: f32,
    pub(super) chase_smoothing: f32,
    pub(super) lens: CameraLens,
}

impl CameraRuntimeState {
    pub(super) fn new(mode: CameraMode, free_camera: Camera, lens: CameraLens) -> Self {
        Self {
            mode,
            free_camera,
            chase_camera_position: free_camera.position,
            chase_distance: 5.5,
            chase_height: 1.3,
            chase_smoothing: 10.0,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SessionNetMode {
    Solo,
    HostLocal,
    JoinLocal,
}

impl SessionNetMode {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Solo => "solo",
            Self::HostLocal => "host-local",
            Self::JoinLocal => "join-local",
        }
    }

    pub(super) fn menu_label(self) -> &'static str {
        match self {
            Self::Solo => "SINGLEPLAYER (OFFLINE)",
            Self::HostLocal => "HOST LOCAL SERVER",
            Self::JoinLocal => "JOIN LOCAL SERVER",
        }
    }

    pub(super) fn prediction_enabled(self) -> bool {
        !matches!(self, Self::Solo)
    }
}

#[derive(Clone, Copy)]
pub(super) struct InputCommandFrame {
    pub(super) sequence: u32,
    pub(super) dt: f32,
    pub(super) movement: MovementInput,
    pub(super) yaw: f32,
    pub(super) pitch: f32,
}

pub(super) struct QueuedServerCommand {
    pub(super) frames_left: u8,
    pub(super) command: InputCommandFrame,
}

pub(super) struct PredictionRuntimeState {
    pub(super) pending_local_inputs: VecDeque<InputCommandFrame>,
    pub(super) next_sequence: u32,
    pub(super) last_authoritative_sequence: u32,
    pub(super) last_position_error: f32,
    pub(super) snap_distance: f32,
    pub(super) correction_gain: f32,
}

impl PredictionRuntimeState {
    pub(super) fn new() -> Self {
        Self {
            pending_local_inputs: VecDeque::new(),
            next_sequence: 1,
            last_authoritative_sequence: 0,
            last_position_error: 0.0,
            snap_distance: 1.2,
            correction_gain: 0.18,
        }
    }
}

pub(super) struct NetcodeRuntimeState {
    pub(super) selected_mode: SessionNetMode,
    pub(super) active_mode: SessionNetMode,
    pub(super) prediction: PredictionRuntimeState,
    pub(super) queued_server_inputs: VecDeque<QueuedServerCommand>,
    pub(super) server_actor: ActorState,
    pub(super) simulated_latency_frames: u8,
}

impl NetcodeRuntimeState {
    pub(super) fn new(local_actor: ActorState) -> Self {
        Self {
            selected_mode: SessionNetMode::Solo,
            active_mode: SessionNetMode::Solo,
            prediction: PredictionRuntimeState::new(),
            queued_server_inputs: VecDeque::new(),
            server_actor: local_actor,
            simulated_latency_frames: 2,
        }
    }

    pub(super) fn reset_for_actor(&mut self, actor: ActorState) {
        self.server_actor = actor;
        self.prediction.pending_local_inputs.clear();
        self.queued_server_inputs.clear();
        self.prediction.last_position_error = 0.0;
        self.prediction.last_authoritative_sequence = 0;
    }

    pub(super) fn set_mode(&mut self, mode: SessionNetMode, actor: ActorState) {
        self.selected_mode = mode;
        self.active_mode = mode;
        self.reset_for_actor(actor);
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
