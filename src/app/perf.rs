use std::time::Instant;

use crate::{
    debug_overlay::{DebugOverlay, OverlayVertex},
    game::world::{TerrainConfig, WORLD_MAX_Y, WORLD_MIN_Y, World},
    render::ChunkRenderKey,
};

use super::{App, random_i32_inclusive};

const STARTUP_PERF_QUERY_SAMPLE_COUNT: usize = 750_000;

#[derive(Clone)]
pub(super) struct StartupPerfPoint {
    label: String,
    chunks_per_sec: f64,
    vertices_per_sec: f64,
}

#[derive(Clone)]
pub(super) struct StartupPerfReport {
    pub(super) points: Vec<StartupPerfPoint>,
    pub(super) query_qps: f64,
}

impl App {
    pub(super) fn run_startup_perf_suite_action(&mut self) {
        let report = self.run_startup_perf_suite();
        self.startup_perf_report = Some(report);
        self.menu.page = super::MenuPage::StartupPerfResults;
        self.menu.index = 0;
        self.menu.hover_index = None;
    }

    fn run_startup_perf_suite(&self) -> StartupPerfReport {
        let world = World::generate_with_terrain_and_seed(
            TerrainConfig::balanced(),
            self.session.world_seed,
        );
        let scenarios = [
            ("LOD0 32", make_perf_lod_keys(32, 0)),
            ("LOD0 64", make_perf_lod_keys(64, 0)),
            ("LOD1 256", make_perf_lod_keys(256, 1)),
            ("LOD2 256", make_perf_lod_keys(256, 2)),
            ("RD128 MIX", make_perf_rd128_mix_keys(128)),
        ];

        let mut points = Vec::with_capacity(scenarios.len());
        for (label, keys) in scenarios {
            let start = Instant::now();
            let mut vertices_total = 0_usize;
            for key in keys.iter().copied() {
                vertices_total += world
                    .build_chunk_mesh_lod(key.origin_chunk, key.lod_level)
                    .len();
            }
            let elapsed_sec = start.elapsed().as_secs_f64().max(f64::EPSILON);
            points.push(StartupPerfPoint {
                label: label.to_string(),
                chunks_per_sec: keys.len() as f64 / elapsed_sec,
                vertices_per_sec: vertices_total as f64 / elapsed_sec,
            });
        }

        StartupPerfReport {
            points,
            query_qps: run_perf_query_probe(&world, STARTUP_PERF_QUERY_SAMPLE_COUNT),
        }
    }

    pub(super) fn startup_perf_graph_overlay(&self, screen_size: [f32; 2]) -> Vec<OverlayVertex> {
        let Some(report) = self.startup_perf_report.as_ref() else {
            return Vec::new();
        };
        if report.points.is_empty() {
            return Vec::new();
        }

        let mut vertices = Vec::new();
        let panel_width = (screen_size[0] * 0.86).clamp(560.0, 1160.0);
        let panel_height = (screen_size[1] * 0.40).clamp(250.0, 420.0);
        let panel_x = ((screen_size[0] - panel_width) * 0.5).max(16.0);
        let panel_y = (screen_size[1] - panel_height - 26.0).max(16.0);
        DebugOverlay::add_quad(
            &mut vertices,
            panel_x,
            panel_y,
            panel_width,
            panel_height,
            [0.04, 0.07, 0.10, 0.90],
        );
        DebugOverlay::add_quad(
            &mut vertices,
            panel_x + 4.0,
            panel_y + 4.0,
            panel_width - 8.0,
            panel_height - 8.0,
            [0.10, 0.14, 0.18, 0.48],
        );

        let title = format!(
            "CHUNK THROUGHPUT GRAPH  QUERY {:.2} M/S",
            report.query_qps / 1_000_000.0
        );
        DebugOverlay::add_text(
            &mut vertices,
            panel_x + 20.0,
            panel_y + 14.0,
            &title,
            2.2,
            [0.94, 0.88, 0.70, 1.0],
        );

        let chart_x = panel_x + 28.0;
        let chart_y = panel_y + 60.0;
        let chart_width = panel_width - 56.0;
        let chart_height = panel_height - 84.0;
        DebugOverlay::add_quad(
            &mut vertices,
            chart_x,
            chart_y,
            chart_width,
            chart_height,
            [0.03, 0.05, 0.07, 0.66],
        );

        let max_chunks_per_sec = report
            .points
            .iter()
            .map(|point| point.chunks_per_sec)
            .fold(0.0_f64, f64::max)
            .max(1.0);
        let slot_width = chart_width / report.points.len() as f32;

        for (index, point) in report.points.iter().enumerate() {
            let x = chart_x + index as f32 * slot_width;
            let bar_width = (slot_width * 0.66).max(8.0);
            let bar_x = x + (slot_width - bar_width) * 0.5;
            let t = (point.chunks_per_sec / max_chunks_per_sec).clamp(0.0, 1.0) as f32;
            let bar_height = t * (chart_height - 28.0).max(1.0);
            let bar_y = chart_y + chart_height - bar_height - 22.0;
            DebugOverlay::add_quad(
                &mut vertices,
                bar_x,
                bar_y,
                bar_width,
                bar_height,
                [0.30, 0.72, 0.76, 0.88],
            );

            let label = point.label.to_ascii_uppercase();
            DebugOverlay::add_text(
                &mut vertices,
                x + 4.0,
                chart_y + chart_height - 16.0,
                &label,
                1.5,
                [0.90, 0.96, 0.94, 1.0],
            );
            let value = format!("{:.0} C/S", point.chunks_per_sec);
            DebugOverlay::add_text(
                &mut vertices,
                bar_x,
                (bar_y - 14.0).max(chart_y + 2.0),
                &value,
                1.5,
                [0.88, 0.94, 0.70, 1.0],
            );
            let vertices_value = if point.vertices_per_sec >= 1_000_000.0 {
                format!("{:.2} MV/S", point.vertices_per_sec / 1_000_000.0)
            } else {
                format!("{:.0} KV/S", point.vertices_per_sec / 1_000.0)
            };
            DebugOverlay::add_text(
                &mut vertices,
                bar_x,
                (bar_y - 27.0).max(chart_y + 2.0),
                &vertices_value,
                1.35,
                [0.74, 0.88, 0.98, 1.0],
            );
        }

        vertices
    }
}

fn make_perf_lod_keys(count: usize, lod_level: u8) -> Vec<ChunkRenderKey> {
    let step = 1_i64 << lod_level;
    let side = (count as f64).sqrt().ceil() as i64 + 3;
    let mut candidates = Vec::new();
    for z in -side..=side {
        for x in -side..=side {
            candidates.push(ChunkRenderKey {
                origin_chunk: (x * step, z * step),
                lod_level,
            });
        }
    }
    candidates.sort_by_key(|key| {
        key.origin_chunk.0 * key.origin_chunk.0 + key.origin_chunk.1 * key.origin_chunk.1
    });
    candidates.truncate(count);
    candidates
}

fn make_perf_rd128_mix_keys(render_distance: i64) -> Vec<ChunkRenderKey> {
    let mut keys = Vec::new();
    super::collect_lod_ring(&mut keys, (0, 0), 0, render_distance.min(16), 0);
    if render_distance > 16 {
        super::collect_lod_ring(&mut keys, (0, 0), 16, render_distance.min(32), 1);
    }
    if render_distance > 32 {
        super::collect_lod_ring(&mut keys, (0, 0), 32, render_distance.min(64), 2);
    }
    if render_distance > 64 {
        super::collect_lod_ring(&mut keys, (0, 0), 64, render_distance, 3);
    }
    keys.sort_by_key(|key| {
        let x = key.origin_chunk.0;
        let z = key.origin_chunk.1;
        (x * x + z * z, key.lod_level)
    });
    keys
}

fn run_perf_query_probe(world: &World, sample_count: usize) -> f64 {
    let mut rng = 0xA5A5_1D2D_3D4D_5D6D_u64;
    let start = Instant::now();
    let mut solids = 0_u64;

    for _ in 0..sample_count {
        let x = random_i32_inclusive(&mut rng, -4096, 4095) as i64;
        let y = random_i32_inclusive(&mut rng, WORLD_MIN_Y, WORLD_MAX_Y);
        let z = random_i32_inclusive(&mut rng, -4096, 4095) as i64;
        if world.is_solid_i64(x, y, z) {
            solids += 1;
        }
    }

    let elapsed_sec = start.elapsed().as_secs_f64().max(f64::EPSILON);
    let _solid_ratio = solids as f64 / sample_count as f64;
    sample_count as f64 / elapsed_sec
}
