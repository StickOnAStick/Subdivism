use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use subdivism::game::{
    asset_registry::{AssetRegistry, BiomeEntry, TextureEntry},
    terrain_recipe::TerrainRecipe,
    world::{TerrainConfig, WORLD_HEIGHT},
};

fn main() {
    let options = ToolOptions::from_env();
    if options.help {
        print_help();
        return;
    }

    if options.has_asset_command() {
        run_asset_command(&options);
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
    assets_file: Option<PathBuf>,
    assets_list: bool,
    assets_add_texture: Option<(String, String)>,
    assets_remove_texture: Option<String>,
    assets_add_biome: Option<(String, String, String, String, String)>,
    assets_remove_biome: Option<String>,
    texture_tint: [f32; 3],
    help: bool,
}

impl ToolOptions {
    fn from_env() -> Self {
        let mut options = Self {
            texture_tint: [1.0, 1.0, 1.0],
            ..Self::default()
        };
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
                "--assets-file" => {
                    if let Some(value) = args.next() {
                        options.assets_file = Some(PathBuf::from(value));
                    }
                }
                "--assets-list" => {
                    options.assets_list = true;
                }
                "--assets-add-texture" => {
                    if let (Some(id), Some(path)) = (args.next(), args.next()) {
                        options.assets_add_texture = Some((id, path));
                    }
                }
                "--assets-remove-texture" => {
                    if let Some(id) = args.next() {
                        options.assets_remove_texture = Some(id);
                    }
                }
                "--assets-add-biome" => {
                    if let (Some(id), Some(profile), Some(top), Some(side), Some(bottom)) = (
                        args.next(),
                        args.next(),
                        args.next(),
                        args.next(),
                        args.next(),
                    ) {
                        options.assets_add_biome = Some((id, profile, top, side, bottom));
                    }
                }
                "--assets-remove-biome" => {
                    if let Some(id) = args.next() {
                        options.assets_remove_biome = Some(id);
                    }
                }
                "--tint" => {
                    if let Some(value) = args.next() {
                        if let Some(parsed) = parse_tint(&value) {
                            options.texture_tint = parsed;
                        }
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

    fn has_asset_command(&self) -> bool {
        self.assets_list
            || self.assets_add_texture.is_some()
            || self.assets_remove_texture.is_some()
            || self.assets_add_biome.is_some()
            || self.assets_remove_biome.is_some()
    }
}

fn run_asset_command(options: &ToolOptions) {
    let path = options
        .assets_file
        .clone()
        .unwrap_or_else(AssetRegistry::default_path);
    let mut registry = AssetRegistry::load_or_default(&path)
        .unwrap_or_else(|err| panic!("failed to load {}: {err}", path.display()));

    if let Some((id, tex_path)) = options.assets_add_texture.as_ref() {
        registry.upsert_texture(TextureEntry {
            id: id.clone(),
            path: tex_path.clone(),
            tint: options.texture_tint,
        });
    }
    if let Some(id) = options.assets_remove_texture.as_ref() {
        registry.remove_texture(id);
    }
    if let Some((id, profile, top, side, bottom)) = options.assets_add_biome.as_ref() {
        registry.upsert_biome(BiomeEntry {
            id: id.clone(),
            terrain_profile: profile.clone(),
            top_texture: top.clone(),
            side_texture: side.clone(),
            bottom_texture: bottom.clone(),
        });
    }
    if let Some(id) = options.assets_remove_biome.as_ref() {
        registry.remove_biome(id);
    }

    if !options.assets_list {
        registry
            .write_to_file(&path)
            .unwrap_or_else(|err| panic!("failed to write {}: {err}", path.display()));
        println!("WROTE {}", path.display());
    }

    println!("TEXTURES {}", registry.textures.len());
    for texture in &registry.textures {
        println!(
            "  {} -> {} tint {:.2},{:.2},{:.2}",
            texture.id, texture.path, texture.tint[0], texture.tint[1], texture.tint[2]
        );
    }
    println!("BIOMES {}", registry.biomes.len());
    for biome in &registry.biomes {
        println!(
            "  {} profile {} textures [{}, {}, {}]",
            biome.id,
            biome.terrain_profile,
            biome.top_texture,
            biome.side_texture,
            biome.bottom_texture
        );
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
        cfg.ravine_strength += 1.2;
    }
    if any(&["desert", "dune", "arid", "dry"]) {
        cfg.desert_base_drop += 3.0;
        cfg.desert_dune_amplitude += 2.0;
        cfg.biome_blend += 0.04;
    }
    if any(&["blend", "blended", "smooth", "transition"]) {
        cfg.biome_blend += 0.06;
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
    cfg.clamp_reasonable();
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
    println!("  --assets-file <p>   asset registry path default terrain/assets.registry");
    println!("  --assets-list       print texture and biome registry");
    println!("  --assets-add-texture <id> <path>   add or replace texture");
    println!("  --assets-remove-texture <id>       remove texture entry");
    println!("  --assets-add-biome <id> <profile> <top> <side> <bottom>");
    println!("  --assets-remove-biome <id>");
    println!("  --tint <r,g,b>      tint used with --assets-add-texture");
    println!("  --help              show this help");
}

fn parse_tint(raw: &str) -> Option<[f32; 3]> {
    let pieces = raw.split(',').map(str::trim).collect::<Vec<_>>();
    if pieces.len() != 3 {
        return None;
    }
    let r = pieces[0].parse::<f32>().ok()?;
    let g = pieces[1].parse::<f32>().ok()?;
    let b = pieces[2].parse::<f32>().ok()?;
    Some([r.clamp(0.0, 2.0), g.clamp(0.0, 2.0), b.clamp(0.0, 2.0)])
}
