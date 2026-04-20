# Terrain Tooling

Create deterministic terrain recipes from a short description:

```bash
cargo run --bin terrain_tool -- --describe "tall alpine mountains with soft valleys" --seed 424242 --out terrain/alpine.terrain
```

Load a generated recipe in-game:

```bash
cargo run -- --terrain-file terrain/alpine.terrain
```

Override the file seed at runtime:

```bash
cargo run -- --terrain-file terrain/alpine.terrain --seed 7
```

Enable in-game dev overlay flags:

```bash
cargo run -- --terrain-file terrain/alpine.terrain --dev
```

## Asset Registry (Textures + Biomes)

Texture and biome entries are managed in `terrain/assets.registry`.

Print current registry:

```bash
cargo run --bin terrain_tool -- --assets-list
```

Add or replace a texture entry:

```bash
cargo run --bin terrain_tool -- --assets-add-texture grass_lush textures/grass_lush.png --tint 1.00,1.06,0.94
```

Remove a texture entry:

```bash
cargo run --bin terrain_tool -- --assets-remove-texture grass_lush
```

Add or replace a biome entry:

```bash
cargo run --bin terrain_tool -- --assets-add-biome highlands alpine grass_top stone stone
```

Remove a biome entry:

```bash
cargo run --bin terrain_tool -- --assets-remove-biome highlands
```

## Terrain Lab (Biome Blending + Ravines)

Use the tuning binary to iterate on layered noise formulas in a contained `32x32` chunk region before shipping a recipe.

Preview blended terrain:

```bash
cargo run --bin terrain_lab
```

In-game Terrain Lab controls:

```text
F8 toggle terrain panel
Arrow Up/Down select parameter
Arrow Left/Right fine tune selected parameter
Page Up/Page Down coarse tune selected parameter
F9 save current terrain preset using current save name
F10 edit save name (Enter saves, Esc cancels)
```

When saving from Terrain Lab, the game writes:
- `terrain/presets/<name>.terrain` for reusable named presets
- `terrain/default.terrain` so normal `cargo run` picks up the saved terrain automatically

Tune multiple dials:

```bash
cargo run --bin terrain_lab -- \
  --cli \
  --set biome_blend=0.24 \
  --set mountain_amplitude=24 \
  --set desert_base_drop=8 \
  --set ravine_strength=4.5 \
  --ravine-offset 96 -48 \
  --csv /tmp/terrain_lab.csv
```

Save directly as a runnable terrain recipe:

```bash
cargo run --bin terrain_lab -- \
  --cli \
  --profile canyon \
  --set biome_blend=0.20 \
  --set desert_dune_amplitude=4.2 \
  --out terrain/canyon_blended.terrain \
  --name canyon_blended
```

List all tunable keys:

```bash
cargo run --bin terrain_lab -- --cli --list-params
```
