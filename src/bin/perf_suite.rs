use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::Path,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use subdivism::{
    game::world::{WORLD_HEIGHT, World},
    render::ChunkRenderKey,
};

const OUTPUT_DIR: &str = "perf";
const LATEST_RESULTS_FILE: &str = "perf/metrics_latest.csv";
const HISTORY_FILE: &str = "perf/metrics_history.csv";
const QUERY_SAMPLE_COUNT: usize = 2_000_000;
const QUICK_MODE_LOD0_LIMIT_FOR_RD128: usize = 96;

#[derive(Clone)]
struct ScenarioResult {
    scenario: String,
    lod_distribution: String,
    chunk_count: usize,
    elapsed_ms: f64,
    chunks_per_sec: f64,
    vertices_total: usize,
    vertices_per_chunk: f64,
    vertices_per_sec: f64,
}

fn main() {
    let full_mode = std::env::args().any(|arg| arg == "--full");
    let run_mode = if full_mode { "full" } else { "quick" };
    let run_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let world = World::generate(64, 32, 64);

    // Warm up cache paths so timed scenarios are more stable.
    warm_up(&world);

    let scenarios = build_scenarios(full_mode);

    let mut results = Vec::new();
    for (name, keys) in scenarios {
        results.push(run_chunk_mesh_scenario(&world, &name, &keys));
    }
    let query_qps = run_world_query_probe(&world, QUERY_SAMPLE_COUNT);

    let peak_chunks_per_sec = results
        .iter()
        .map(|r| r.chunks_per_sec)
        .fold(0.0_f64, f64::max);
    let peak_vertices_per_sec = results
        .iter()
        .map(|r| r.vertices_per_sec)
        .fold(0.0_f64, f64::max);
    let hw_threads = std::thread::available_parallelism()
        .map(|n| n.get() as f64)
        .unwrap_or(1.0);
    let theoretical_chunks_per_sec = peak_chunks_per_sec * hw_threads;
    let theoretical_vertices_per_sec = peak_vertices_per_sec * hw_threads;
    let previous = read_previous_chunks_per_sec(LATEST_RESULTS_FILE);

    write_latest_results(
        run_epoch,
        run_mode,
        &results,
        query_qps,
        hw_threads,
        theoretical_chunks_per_sec,
        theoretical_vertices_per_sec,
    );
    append_history(
        run_epoch,
        run_mode,
        &results,
        query_qps,
        hw_threads,
        theoretical_chunks_per_sec,
        theoretical_vertices_per_sec,
    );

    println!("Perf suite complete.");
    println!("Mode: {}", run_mode);
    println!("Use `cargo run --bin perf_suite -- --full` for full coverage.");
    println!("Wrote {}", LATEST_RESULTS_FILE);
    println!("Wrote {}", HISTORY_FILE);
    println!(
        "Peak measured: {:.2} chunks/s, {:.2} M verts/s",
        peak_chunks_per_sec,
        peak_vertices_per_sec / 1_000_000.0
    );
    println!(
        "Theoretical max (x{:.0} threads): {:.2} chunks/s, {:.2} M verts/s",
        hw_threads,
        theoretical_chunks_per_sec,
        theoretical_vertices_per_sec / 1_000_000.0
    );
    println!(
        "World query throughput: {:.2} M queries/s",
        query_qps / 1_000_000.0
    );

    for result in &results {
        let delta = previous
            .get(&result.scenario)
            .map(|prev| result.chunks_per_sec - prev)
            .unwrap_or(0.0);
        println!(
            "{}: {:.2} ms for {} chunks ({:.2} chunks/s, delta {:+.2})",
            result.scenario, result.elapsed_ms, result.chunk_count, result.chunks_per_sec, delta
        );
    }
}

fn build_scenarios(full_mode: bool) -> Vec<(String, Vec<ChunkRenderKey>)> {
    if full_mode {
        vec![
            ("lod0_256".to_string(), make_lod_keys(256, 0)),
            ("lod0_1024".to_string(), make_lod_keys(1024, 0)),
            ("lod1_1024".to_string(), make_lod_keys(1024, 1)),
            ("lod2_1024".to_string(), make_lod_keys(1024, 2)),
            ("lod3_2048".to_string(), make_lod_keys(2048, 3)),
            ("rd128_mix".to_string(), make_rd128_mix_keys(128)),
        ]
    } else {
        vec![
            ("lod0_32".to_string(), make_lod_keys(32, 0)),
            ("lod0_64".to_string(), make_lod_keys(64, 0)),
            ("lod1_1024".to_string(), make_lod_keys(1024, 1)),
            ("lod2_1024".to_string(), make_lod_keys(1024, 2)),
            ("lod3_2048".to_string(), make_lod_keys(2048, 3)),
            (
                "rd128_mix_quick".to_string(),
                make_rd128_mix_keys_quick(128, QUICK_MODE_LOD0_LIMIT_FOR_RD128),
            ),
        ]
    }
}

fn warm_up(world: &World) {
    for key in make_lod_keys(32, 0) {
        let _ = world.build_chunk_mesh_lod(key.origin_chunk, key.lod_level);
    }
}

fn run_chunk_mesh_scenario(world: &World, scenario: &str, keys: &[ChunkRenderKey]) -> ScenarioResult {
    let mut lod_counts = [0_u32; 8];
    let start = Instant::now();
    let mut vertices_total = 0_usize;

    for key in keys {
        lod_counts[key.lod_level as usize] += 1;
        let vertices = world.build_chunk_mesh_lod(key.origin_chunk, key.lod_level);
        vertices_total += vertices.len();
    }

    let elapsed = start.elapsed();
    let elapsed_sec = elapsed.as_secs_f64().max(f64::EPSILON);
    let chunk_count = keys.len();
    let chunk_count_f64 = chunk_count as f64;
    let lod_distribution = lod_counts
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > 0)
        .map(|(lod, count)| format!("L{}={}", lod, count))
        .collect::<Vec<_>>()
        .join("|");

    ScenarioResult {
        scenario: scenario.to_string(),
        lod_distribution,
        chunk_count,
        elapsed_ms: elapsed_sec * 1000.0,
        chunks_per_sec: chunk_count_f64 / elapsed_sec,
        vertices_total,
        vertices_per_chunk: vertices_total as f64 / chunk_count_f64.max(1.0),
        vertices_per_sec: vertices_total as f64 / elapsed_sec,
    }
}

fn run_world_query_probe(world: &World, sample_count: usize) -> f64 {
    let mut rng = 0xA5A5_1D2D_3D4D_5D6D_u64;
    let start = Instant::now();
    let mut solids = 0_u64;

    for _ in 0..sample_count {
        let x = (rand_u64(&mut rng) % 8192) as i64 - 4096;
        let y = (rand_u64(&mut rng) % WORLD_HEIGHT as u64) as i32;
        let z = (rand_u64(&mut rng) % 8192) as i64 - 4096;
        if world.is_solid_i64(x, y, z) {
            solids += 1;
        }
    }

    let elapsed_sec = start.elapsed().as_secs_f64().max(f64::EPSILON);
    let _solid_ratio = solids as f64 / sample_count as f64;
    sample_count as f64 / elapsed_sec
}

fn make_lod_keys(count: usize, lod_level: u8) -> Vec<ChunkRenderKey> {
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

fn make_rd128_mix_keys(render_distance: i64) -> Vec<ChunkRenderKey> {
    let mut keys = Vec::new();
    collect_lod_ring(&mut keys, (0, 0), 0, render_distance.min(16), 0);
    if render_distance > 16 {
        collect_lod_ring(&mut keys, (0, 0), 16, render_distance.min(32), 1);
    }
    if render_distance > 32 {
        collect_lod_ring(&mut keys, (0, 0), 32, render_distance.min(64), 2);
    }
    if render_distance > 64 {
        collect_lod_ring(&mut keys, (0, 0), 64, render_distance, 3);
    }
    keys.sort_by_key(|key| {
        let x = key.origin_chunk.0;
        let z = key.origin_chunk.1;
        (x * x + z * z, key.lod_level)
    });
    keys
}

fn make_rd128_mix_keys_quick(render_distance: i64, lod0_limit: usize) -> Vec<ChunkRenderKey> {
    let mut keys = make_rd128_mix_keys(render_distance);
    if lod0_limit == 0 {
        return keys.into_iter().filter(|k| k.lod_level > 0).collect();
    }
    let mut kept_lod0 = 0_usize;
    keys.retain(|key| {
        if key.lod_level != 0 {
            return true;
        }
        if kept_lod0 < lod0_limit {
            kept_lod0 += 1;
            true
        } else {
            false
        }
    });
    keys
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
    let min_sq = min_radius * min_radius;
    let max_sq = max_radius * max_radius;
    let start_x = (center_chunk.0 - max_radius).div_euclid(step) * step;
    let end_x = (center_chunk.0 + max_radius).div_euclid(step) * step;
    let start_z = (center_chunk.1 - max_radius).div_euclid(step) * step;
    let end_z = (center_chunk.1 + max_radius).div_euclid(step) * step;
    let half_step = step as f32 * 0.5;
    let mut seen = HashSet::new();

    let mut origin_z = start_z;
    while origin_z <= end_z {
        let mut origin_x = start_x;
        while origin_x <= end_x {
            let center_x = origin_x as f32 + half_step;
            let center_z = origin_z as f32 + half_step;
            let dx = center_x - center_chunk.0 as f32;
            let dz = center_z - center_chunk.1 as f32;
            let dist_sq = dx * dx + dz * dz;
            let in_min = if min_radius == 0 {
                dist_sq >= 0.0
            } else {
                dist_sq > min_sq as f32
            };

            if dist_sq <= max_sq as f32 && in_min {
                let key = ChunkRenderKey {
                    origin_chunk: (origin_x, origin_z),
                    lod_level,
                };
                if seen.insert(key) {
                    out.push(key);
                }
            }
            origin_x += step;
        }
        origin_z += step;
    }
}

fn write_latest_results(
    run_epoch: u64,
    run_mode: &str,
    results: &[ScenarioResult],
    query_qps: f64,
    hw_threads: f64,
    theoretical_chunks_per_sec: f64,
    theoretical_vertices_per_sec: f64,
) {
    fs::create_dir_all(OUTPUT_DIR).expect("failed to create perf output directory");
    let mut file = File::create(LATEST_RESULTS_FILE).expect("failed to create latest results file");
    writeln!(
        file,
        "run_epoch,mode,scenario,lod_distribution,chunks,elapsed_ms,chunks_per_sec,vertices_total,vertices_per_chunk,vertices_per_sec,query_qps,hw_threads,theoretical_chunks_per_sec,theoretical_vertices_per_sec"
    )
    .expect("failed to write header");

    for result in results {
        writeln!(
            file,
            "{},{},{},{},{},{:.4},{:.4},{},{:.4},{:.4},{:.4},{:.0},{:.4},{:.4}",
            run_epoch,
            run_mode,
            result.scenario,
            result.lod_distribution,
            result.chunk_count,
            result.elapsed_ms,
            result.chunks_per_sec,
            result.vertices_total,
            result.vertices_per_chunk,
            result.vertices_per_sec,
            query_qps,
            hw_threads,
            theoretical_chunks_per_sec,
            theoretical_vertices_per_sec
        )
        .expect("failed to write scenario row");
    }
}

fn append_history(
    run_epoch: u64,
    run_mode: &str,
    results: &[ScenarioResult],
    query_qps: f64,
    hw_threads: f64,
    theoretical_chunks_per_sec: f64,
    theoretical_vertices_per_sec: f64,
) {
    fs::create_dir_all(OUTPUT_DIR).expect("failed to create perf output directory");
    const HISTORY_HEADER: &str = "run_epoch,mode,peak_chunks_per_sec,peak_vertices_per_sec,theoretical_chunks_per_sec,theoretical_vertices_per_sec,rd128_elapsed_ms,rd128_chunks,query_qps,hw_threads";
    let mut needs_header = true;
    if Path::new(HISTORY_FILE).exists() {
        if let Ok(file) = File::open(HISTORY_FILE) {
            let mut reader = BufReader::new(file);
            let mut header_line = String::new();
            if reader.read_line(&mut header_line).is_ok() && header_line.trim() == HISTORY_HEADER {
                needs_header = false;
            } else {
                let backup_path = format!("{}.legacy_{}", HISTORY_FILE, run_epoch);
                let _ = fs::rename(HISTORY_FILE, backup_path);
                needs_header = true;
            }
        }
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(HISTORY_FILE)
        .expect("failed to open history file");

    if needs_header {
        writeln!(file, "{}", HISTORY_HEADER).expect("failed to write history header");
    }

    let peak_chunks = results
        .iter()
        .map(|r| r.chunks_per_sec)
        .fold(0.0_f64, f64::max);
    let peak_vertices = results
        .iter()
        .map(|r| r.vertices_per_sec)
        .fold(0.0_f64, f64::max);
    let rd128 = results
        .iter()
        .find(|r| r.scenario == "rd128_mix" || r.scenario == "rd128_mix_quick");
    let rd128_elapsed_ms = rd128.map(|r| r.elapsed_ms).unwrap_or(0.0);
    let rd128_chunks = rd128.map(|r| r.chunk_count).unwrap_or(0);

    writeln!(
        file,
        "{},{},{:.4},{:.4},{:.4},{:.4},{:.4},{},{:.4},{:.0}",
        run_epoch,
        run_mode,
        peak_chunks,
        peak_vertices,
        theoretical_chunks_per_sec,
        theoretical_vertices_per_sec,
        rd128_elapsed_ms,
        rd128_chunks,
        query_qps,
        hw_threads
    )
    .expect("failed to append history row");
}

fn read_previous_chunks_per_sec(path: &str) -> HashMap<String, f64> {
    let mut map = HashMap::new();
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return map,
    };
    let reader = BufReader::new(file);
    for (index, line) in reader.lines().enumerate() {
        let Ok(line) = line else {
            continue;
        };
        if index == 0 || line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() < 7 {
            continue;
        }
        if let Ok(chunks_per_sec) = cols[6].parse::<f64>() {
            map.insert(cols[2].to_string(), chunks_per_sec);
        }
    }
    map
}

fn rand_u64(state: &mut u64) -> u64 {
    *state ^= *state >> 12;
    *state ^= *state << 25;
    *state ^= *state >> 27;
    state.wrapping_mul(0x2545_F491_4F6C_DD1D)
}
