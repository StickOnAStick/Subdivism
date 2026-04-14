use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use subdivism::game::{
    terrain_recipe::TerrainRecipe,
    world::{TerrainConfig, WORLD_HEIGHT},
};

fn main() {
    let options = ToolOptions::from_env();
    if options.help {
        print_help();
        return;
    }

    let seed = options.seed.unwrap_or_else(default_seed);
    let mut terrain = options
        .profile
        .as_deref()
        .and_then(TerrainConfig::from_profile_name)
        .unwrap_or_else(TerrainConfig::balanced);
    if !options.description.is_empty() {
        apply_description(&mut terrain, &options.description);
    }

    let mut recipe = TerrainRecipe::balanced(seed);
    recipe.name = options
        .name
        .clone()
        .unwrap_or_else(|| slugify(&options.description, "custom_terrain"));
    recipe.description = options.description.clone();
    recipe.profile = options
        .profile
        .clone()
        .unwrap_or_else(|| "custom".to_string());
    recipe.seed = seed;
    recipe.terrain = terrain;

    if let Some(path) = options.out.as_ref() {
        recipe
            .write_to_file(path)
            .unwrap_or_else(|err| panic!("failed to write recipe {}: {err}", path.display()));
        println!("WROTE {}", path.display());
        println!(
            "RUN GAME WITH  cargo run -- --terrain-file {}",
            path.display()
        );
        println!(
            "OPTIONAL SEED OVERRIDE  cargo run -- --terrain-file {} --seed 12345",
            path.display()
        );
    } else {
        println!("NO OUTPUT FILE PROVIDED  USE --out TO SAVE A RECIPE");
    }

    println!();
    println!("RECIPE PREVIEW");
    println!("{}", recipe.to_kv_text());
}

#[derive(Default)]
struct ToolOptions {
    description: String,
    profile: Option<String>,
    seed: Option<i64>,
    out: Option<PathBuf>,
    name: Option<String>,
    help: bool,
}

impl ToolOptions {
    fn from_env() -> Self {
        let mut options = Self::default();
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--describe" => {
                    if let Some(value) = args.next() {
                        options.description = value;
                    }
                }
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
                "--out" => {
                    if let Some(value) = args.next() {
                        options.out = Some(PathBuf::from(value));
                    }
                }
                "--name" => {
                    if let Some(value) = args.next() {
                        options.name = Some(value);
                    }
                }
                "--help" | "-h" => {
                    options.help = true;
                }
                _ => {}
            }
        }
        options
    }
}

fn default_seed() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn apply_description(cfg: &mut TerrainConfig, description: &str) {
    let words = description
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect::<Vec<_>>();

    let has = |term: &str| words.iter().any(|word| word == term);
    let any = |terms: &[&str]| terms.iter().any(|term| has(term));

    if any(&["mountain", "mountains", "peak", "peaks", "alpine", "rugged"]) {
        cfg.mountain_amplitude += 8.0;
        cfg.macro_amplitude += 2.0;
        cfg.valley_depth -= 1.5;
    }
    if any(&["valley", "valleys", "basin", "river"]) {
        cfg.valley_depth += 4.0;
        cfg.macro_amplitude += 1.0;
    }
    if any(&["canyon", "canyons", "cliff", "cliffs", "mesa", "ravine"]) {
        cfg.cliff_strength += 0.24;
        cfg.terrace_step += 0.8;
        cfg.valley_depth += 3.0;
    }
    if any(&["flat", "plain", "plains", "meadow", "gentle"]) {
        cfg.macro_amplitude *= 0.55;
        cfg.mountain_amplitude *= 0.38;
        cfg.valley_depth *= 0.5;
        cfg.detail_amplitude *= 0.75;
    }
    if any(&["rolling", "hill", "hills", "undulating"]) {
        cfg.macro_amplitude += 3.0;
        cfg.detail_amplitude += 0.8;
        cfg.mountain_amplitude += 1.5;
    }
    if any(&["sharp", "steep", "dramatic", "extreme"]) {
        cfg.cliff_strength += 0.12;
        cfg.mountain_amplitude *= 1.15;
    }
    if any(&["smooth", "soft"]) {
        cfg.cliff_strength *= 0.66;
        cfg.detail_amplitude *= 0.72;
    }

    cfg.base_height = cfg.base_height.clamp(8.0, (WORLD_HEIGHT - 8) as f32);
    cfg.macro_scale = cfg.macro_scale.clamp(0.003, 0.08);
    cfg.detail_scale = cfg.detail_scale.clamp(0.01, 0.25);
    cfg.mountain_scale = cfg.mountain_scale.clamp(0.0025, 0.08);
    cfg.valley_scale = cfg.valley_scale.clamp(0.0025, 0.08);
    cfg.cliff_scale = cfg.cliff_scale.clamp(0.005, 0.12);

    cfg.macro_amplitude = cfg.macro_amplitude.clamp(0.0, 24.0);
    cfg.detail_amplitude = cfg.detail_amplitude.clamp(0.0, 10.0);
    cfg.mountain_amplitude = cfg.mountain_amplitude.clamp(0.0, 28.0);
    cfg.valley_depth = cfg.valley_depth.clamp(0.0, 20.0);
    cfg.cliff_strength = cfg.cliff_strength.clamp(0.0, 1.0);
    cfg.terrace_step = cfg.terrace_step.clamp(0.5, 8.0);
}

fn slugify(text: &str, fallback: &str) -> String {
    let slug = text
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|part| !part.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("_");
    if slug.is_empty() {
        fallback.to_string()
    } else {
        slug
    }
}

fn print_help() {
    println!("terrain_tool usage");
    println!(
        "  cargo run --bin terrain_tool -- --describe \"alpine cliffs\" --seed 42 --out terrain/alpine.terrain"
    );
    println!("options");
    println!("  --describe <text>   freeform terrain description");
    println!("  --profile <name>    base profile balanced alpine canyon valleylands");
    println!("  --seed <int>        deterministic world seed");
    println!("  --out <path>        output recipe file");
    println!("  --name <id>         recipe name");
    println!("  --help              show this help");
}
