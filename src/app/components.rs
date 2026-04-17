use std::collections::{HashMap, HashSet};

use crate::render::ChunkRenderKey;

use super::{
    ChunkBuildPipeline, DEFAULT_RENDER_DISTANCE_CHUNKS, MenuPage, UiMode, World,
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
}

impl TerrainLabState {
    pub(super) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            panel_visible: enabled,
            selected_index: 0,
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
