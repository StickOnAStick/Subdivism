# Subdivism

Subdivism is a Rust voxel sandbox prototype with:
- Procedural terrain generation (multi-biome, cave carving, ravines, LOD terrain meshing)
- In-game block editing, including sub-voxel placement/removal (3x3 and 6x6 subdivision)
- Runtime overlays for HUD, pause/settings, inventory, and Terrain Lab tuning
- Offline tooling for terrain recipes and performance benchmarks

## Getting Started

### 1) Prerequisites
- Rust toolchain (`cargo`, `rustc`)
- A GPU/driver stack that supports `wgpu` on your platform

### 2) Run the game
```bash
cargo run
```

### 3) Useful launch options
```bash
# Load a terrain recipe file
cargo run -- --terrain-file terrain/alpine.terrain

# Override seed at runtime
cargo run -- --terrain-file terrain/alpine.terrain --seed 7

# Enable dev mode (adds reload/debug affordances)
cargo run -- --dev
```

## Core Controls

- `WASD`: Move
- `Space`: Jump / fly up in free camera
- `Shift`: Sprint / fly down in free camera
- `Mouse`: Look
- `Left Click`: Remove block
- `Right Click`: Place block
- `Ctrl + Left/Right Click`: Cycle subdivision scale
- `Mouse Wheel` (while aiming at a block): Cycle subdivision scale
- `1..8`: Select hotbar slot
- `E`: Inventory UI
- `Esc`: Pause menu
- `F3`: Debug overlay
- `F4`: Frame cap cycle
- `F5`: Toggle player/free camera
- `F6`: Reload terrain recipe in dev mode

Terrain Lab-specific controls:
- `F8`: Toggle Terrain Lab panel
- `F9`: Save Terrain Lab preset using current save name
- `F10`: Edit Terrain Lab save name (`Enter` save, `Esc` cancel)

Terrain Lab saves now write both:
- `terrain/presets/<name>.terrain`
- `terrain/default.terrain` (auto-import path used by `cargo run`)

Graphics/runtime settings now persist to:
- `settings/user.settings` (auto-loaded on startup, auto-saved when changed in-game)

## Project Layout

- `src/`
  - Core app loop, rendering, camera, overlays, mesh types
  - See [src/README.md](src/README.md)
- `src/game/`
  - World/terrain model, physics, inventory, interactions, recipes, prefabs
  - See [src/game/README.md](src/game/README.md)
- `src/bin/`
  - Tooling binaries (`terrain_tool`, `terrain_lab`, `perf_suite`)
  - See [src/bin/README.md](src/bin/README.md)
- `terrain/`
  - Terrain recipes and asset registry
  - See [terrain/README.md](terrain/README.md)
- `perf/`
  - Performance result CSVs and benchmark notes
  - See [perf/README.md](perf/README.md)
- `docs/`
  - Architecture and maintenance docs
  - [Game architecture overview](docs/game_architecture_overview.md)
  - [Design overview](docs/design_overview.md)
  - [Loose ends and cleanup plan](docs/loose_ends.md)

## Additional Commands

### Terrain recipe helper
```bash
cargo run --bin terrain_tool -- --describe "tall alpine mountains with soft valleys" --seed 424242 --out terrain/alpine.terrain
```

### Terrain Lab
```bash
# Interactive in-game Terrain Lab
cargo run --bin terrain_lab

# CLI terrain sampling mode
cargo run --bin terrain_lab -- --cli --profile balanced --ascii
```

### Performance suite
```bash
# Quick suite
cargo run --bin perf_suite

# Full suite
cargo run --bin perf_suite -- --full
```
