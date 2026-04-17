use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use subdivism::game::{
    terrain_recipe::TerrainRecipe,
    world::{CHUNK_SIZE, TerrainConfig, World},
};

fn main() {
    if let Err(err) = try_main() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn try_main() -> Result<(), String> {
    let raw_args = std::env::args().skip(1).collect::<Vec<_>>();
    let wants_cli = raw_args.iter().any(|arg| arg == "--cli");
    let wants_help = raw_args.iter().any(|arg| arg == "--help" || arg == "-h");
    if !wants_cli && !wants_help {
        subdivism::app::run_terrain_lab();
        return Ok(());
    }

    let options = LabOptions::from_env();
    if options.help {
        print_help();
        return Ok(());
    }
    if options.list_params {
        println!("TERRAIN PARAMETERS");
        for key in TerrainConfig::parameter_keys() {
            println!("  {key}");
        }
        return Ok(());
    }

    let seed = options.seed.unwrap_or_else(default_seed);
    let mut terrain = options
        .profile
        .as_deref()
        .and_then(TerrainConfig::from_profile_name)
        .unwrap_or_else(TerrainConfig::balanced);

    for (key, value) in &options.sets {
        terrain
            .set_named_param(key, *value)
            .map_err(|err| format!("failed to apply --set {key}={value}: {err}"))?;
    }
    if let Some((x, z)) = options.ravine_offset {
        terrain.ravine_offset_x = x;
        terrain.ravine_offset_z = z;
        terrain.clamp_reasonable();
    }

    let world = World::generate_with_terrain_and_seed(terrain, seed);
    let sample = sample_region(
        &world,
        options.center_chunk.0,
        options.center_chunk.1,
        options.region_chunks,
    );
    println!(
        "REGION {}x{} chunks around chunk ({}, {})",
        options.region_chunks,
        options.region_chunks,
        options.center_chunk.0,
        options.center_chunk.1
    );
    println!(
        "HEIGHT STATS min={} max={} avg={:.2} stddev={:.2}",
        sample.min_height, sample.max_height, sample.mean_height, sample.stddev
    );
    println!(
        "SLOPE AVG {:.3} MAX {:.3}",
        sample.mean_slope, sample.max_slope
    );
    if options.ascii {
        println!();
        println!("ASCII HEIGHT PREVIEW");
        println!("{}", sample.ascii);
    }
    if let Some(csv_path) = options.csv_path.as_ref() {
        fs::write(csv_path, sample.csv)
            .map_err(|err| format!("failed to write sample CSV {}: {err}", csv_path.display()))?;
        println!("WROTE CSV {}", csv_path.display());
    }

    if let Some(out_path) = options.out.as_ref() {
        let mut recipe = TerrainRecipe::balanced(seed);
        recipe.name = options
            .name
            .clone()
            .unwrap_or_else(|| format!("lab_{}", sanitize_name(&options.profile_name())));
        recipe.description = format!(
            "terrain_lab {} region={}x{} center=({}, {})",
            profile_or_custom(&options),
            options.region_chunks,
            options.region_chunks,
            options.center_chunk.0,
            options.center_chunk.1
        );
        recipe.profile = options.profile_name();
        recipe.seed = seed;
        recipe.terrain = terrain;
        recipe
            .write_to_file(out_path)
            .map_err(|err| format!("failed to write recipe {}: {err}", out_path.display()))?;
        println!("WROTE RECIPE {}", out_path.display());
        println!(
            "RUN GAME WITH cargo run -- --terrain-file {}",
            out_path.display()
        );
    }
    Ok(())
}

#[derive(Default)]
struct LabOptions {
    profile: Option<String>,
    seed: Option<i64>,
    name: Option<String>,
    out: Option<PathBuf>,
    csv_path: Option<PathBuf>,
    center_chunk: (i64, i64),
    region_chunks: i64,
    sets: Vec<(String, f32)>,
    ravine_offset: Option<(f32, f32)>,
    ascii: bool,
    list_params: bool,
    help: bool,
}

impl LabOptions {
    fn from_env() -> Self {
        let mut options = Self {
            region_chunks: 32,
            ..Self::default()
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--profile" => {
                    if let Some(value) = args.next() {
                        options.profile = Some(value);
                    }
                }
                "--seed" => {
                    if let Some(value) = args.next() {
                        options.seed = value.parse::<i64>().ok();
                    }
                }
                "--name" => {
                    if let Some(value) = args.next() {
                        options.name = Some(value);
                    }
                }
                "--out" => {
                    if let Some(value) = args.next() {
                        options.out = Some(PathBuf::from(value));
                    }
                }
                "--csv" => {
                    if let Some(value) = args.next() {
                        options.csv_path = Some(PathBuf::from(value));
                    }
                }
                "--center-chunk" => {
                    if let (Some(x), Some(z)) = (args.next(), args.next()) {
                        if let (Ok(x), Ok(z)) = (x.parse::<i64>(), z.parse::<i64>()) {
                            options.center_chunk = (x, z);
                        }
                    }
                }
                "--region-chunks" => {
                    if let Some(value) = args.next() {
                        if let Ok(value) = value.parse::<i64>() {
                            options.region_chunks = value.clamp(1, 128);
                        }
                    }
                }
                "--set" => {
                    if let Some(value) = args.next() {
                        if let Some((key, raw)) = value.split_once('=') {
                            if let Ok(parsed) = raw.parse::<f32>() {
                                options.sets.push((key.to_string(), parsed));
                            }
                        }
                    }
                }
                "--ravine-offset" => {
                    if let (Some(x), Some(z)) = (args.next(), args.next()) {
                        if let (Ok(x), Ok(z)) = (x.parse::<f32>(), z.parse::<f32>()) {
                            options.ravine_offset = Some((x, z));
                        }
                    }
                }
                "--ascii" => {
                    options.ascii = true;
                }
                "--list-params" => {
                    options.list_params = true;
                }
                "--help" | "-h" => {
                    options.help = true;
                }
                "--cli" => {}
                _ => {}
            }
        }
        options
    }

    fn profile_name(&self) -> String {
        self.profile.clone().unwrap_or_else(|| "custom".to_string())
    }
}

struct RegionSample {
    min_height: i32,
    max_height: i32,
    mean_height: f32,
    stddev: f32,
    mean_slope: f32,
    max_slope: f32,
    ascii: String,
    csv: String,
}

fn sample_region(
    world: &World,
    center_chunk_x: i64,
    center_chunk_z: i64,
    region_chunks: i64,
) -> RegionSample {
    let span_blocks = region_chunks * CHUNK_SIZE;
    let min_x = center_chunk_x * CHUNK_SIZE - span_blocks / 2;
    let min_z = center_chunk_z * CHUNK_SIZE - span_blocks / 2;
    let width = span_blocks.max(1) as usize;
    let mut heights = vec![0_i32; width * width];

    let mut min_height = i32::MAX;
    let mut max_height = i32::MIN;
    let mut sum = 0.0_f64;
    for dz in 0..width {
        for dx in 0..width {
            let wx = min_x + dx as i64;
            let wz = min_z + dz as i64;
            let h = world.surface_height_at(wx, wz);
            heights[dz * width + dx] = h;
            min_height = min_height.min(h);
            max_height = max_height.max(h);
            sum += h as f64;
        }
    }
    let sample_count = (width * width) as f64;
    let mean = if sample_count > 0.0 {
        sum / sample_count
    } else {
        0.0
    };
    let mut variance_sum = 0.0_f64;
    for h in &heights {
        let delta = *h as f64 - mean;
        variance_sum += delta * delta;
    }
    let stddev = if sample_count > 0.0 {
        (variance_sum / sample_count).sqrt() as f32
    } else {
        0.0
    };

    let mut slope_sum = 0.0_f64;
    let mut slope_count = 0_u64;
    let mut max_slope = 0.0_f32;
    for z in 0..width.saturating_sub(1) {
        for x in 0..width.saturating_sub(1) {
            let h = heights[z * width + x] as f32;
            let hx = heights[z * width + x + 1] as f32;
            let hz = heights[(z + 1) * width + x] as f32;
            let slope = ((hx - h).abs() + (hz - h).abs()) * 0.5;
            slope_sum += slope as f64;
            slope_count += 1;
            max_slope = max_slope.max(slope);
        }
    }
    let mean_slope = if slope_count > 0 {
        (slope_sum / slope_count as f64) as f32
    } else {
        0.0
    };

    let ascii = ascii_preview(&heights, width, min_height, max_height);
    let csv = csv_preview(&heights, width);

    RegionSample {
        min_height,
        max_height,
        mean_height: mean as f32,
        stddev,
        mean_slope,
        max_slope,
        ascii,
        csv,
    }
}

fn ascii_preview(heights: &[i32], width: usize, min_height: i32, max_height: i32) -> String {
    const CHARSET: &[u8] = b" .:-=+*#%@";
    let preview_w = width.min(120);
    let preview_h = width.min(64);
    let step_x = (width as f32 / preview_w as f32).max(1.0);
    let step_z = (width as f32 / preview_h as f32).max(1.0);
    let range = (max_height - min_height).max(1) as f32;
    let mut out = String::new();
    for row in 0..preview_h {
        let z = (row as f32 * step_z).floor() as usize;
        for col in 0..preview_w {
            let x = (col as f32 * step_x).floor() as usize;
            let idx = z.min(width - 1) * width + x.min(width - 1);
            let h = heights[idx] as f32;
            let t = ((h - min_height as f32) / range).clamp(0.0, 1.0);
            let ci = (t * (CHARSET.len() - 1) as f32).round() as usize;
            out.push(CHARSET[ci] as char);
        }
        out.push('\n');
    }
    out
}

fn csv_preview(heights: &[i32], width: usize) -> String {
    let mut out = String::new();
    for z in 0..width {
        for x in 0..width {
            if x > 0 {
                out.push(',');
            }
            out.push_str(&heights[z * width + x].to_string());
        }
        out.push('\n');
    }
    out
}

fn sanitize_name(raw: &str) -> String {
    let value = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    let joined = value
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if joined.is_empty() {
        "terrain".to_string()
    } else {
        joined
    }
}

fn profile_or_custom(options: &LabOptions) -> &str {
    options.profile.as_deref().unwrap_or("custom")
}

fn default_seed() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn print_help() {
    println!("terrain_lab usage");
    println!("  cargo run --bin terrain_lab");
    println!("  launches interactive in-game Terrain Lab");
    println!("  use --cli for offline sampling commands");
    println!("  cargo run --bin terrain_lab -- --cli --profile balanced --ascii");
    println!(
        "  cargo run --bin terrain_lab -- --cli --set mountain_amplitude=22 --set biome_blend=0.22"
    );
    println!(
        "  cargo run --bin terrain_lab -- --cli --region-chunks 32 --center-chunk 0 0 --ravine-offset 120 -40"
    );
    println!(
        "  cargo run --bin terrain_lab -- --cli --out terrain/lab_custom.terrain --name my_biome_mix"
    );
    println!("options");
    println!("  --profile <name>         base profile balanced alpine canyon valleylands");
    println!("  --seed <int>             deterministic seed");
    println!("  --set <key=value>        tune a TerrainConfig field");
    println!("  --list-params            print all tunable keys");
    println!("  --ravine-offset <x> <z>  custom ravine offset for local region tests");
    println!("  --region-chunks <n>      sample window size in chunks (default 32)");
    println!("  --center-chunk <x> <z>   sample region center chunk (default 0 0)");
    println!("  --ascii                  print ASCII height map preview");
    println!("  --csv <path>             write sampled height map CSV");
    println!("  --out <path>             save terrain recipe file");
    println!("  --name <id>              recipe name");
}
