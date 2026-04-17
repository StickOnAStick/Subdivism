# `src/game/` Gameplay and World Systems

This folder contains world simulation and gameplay-focused systems.

## Modules and Responsibilities

- `world.rs`
  - Core voxel world model
  - Procedural terrain + biome blending + cave/ravine shaping
  - Mutable block/sub-block overrides
  - Chunk meshing (`LOD0..LOD3`) and simple light preprocessing

- `actor.rs`
  - Actor state containers (`ActorState`, `ActorRoster`)
  - Player camera derivation from actor look/motion

- `physics.rs`
  - Movement integration and collision against full blocks + sub-blocks
  - Gravity/jump/sprint config via `PhysicsConfig`

- `inventory.rs`
  - Hotbar/backpack slot model
  - Stack insertion/removal and backpack-to-hotbar transfer

- `interact.rs`
  - World raycast helpers
  - Face-plane/sub-grid helpers for placement and subdivision overlays
  - World/screen projection helpers for overlays

- `hud.rs`
  - Label/color/formatting helpers for HUD + inventory text rows

- `terrain_recipe.rs`
  - Terrain recipe serialization/deserialization (KV text format)
  - Seed/profile/parameter persistence

- `terrain_params.rs`
  - Terrain Lab parameter metadata (min/max/step/label/value access)

- `asset_registry.rs`
  - CSV-like registry parser/writer for textures + biome mappings
  - Used by tooling currently; not fully wired into render materials yet

- `prefab.rs`
  - Prefab archetype catalog and deterministic spawn candidate logic
  - Presently decoupled from runtime world population

## Main Data Model Snapshot

```text
World
  overrides:    HashMap<BlockPos, Block>
  sub_overrides:HashMap<SubBlockPos, Block>
  terrain:      TerrainConfig
  seed:         i64
```

## Editing Flow

```text
raycast -> choose target cell/sub-cell -> mutate World override map
       -> mark affected chunk keys dirty -> async remesh -> GPU upload
```

## Notable Quirks

- `sub_blocks_in_cell` currently filters the entire sub-block map per call.
- `World` combines generation/storage/meshing concerns and is a strong candidate for decomposition.
