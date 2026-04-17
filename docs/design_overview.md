# Design Overview

This document explains how Subdivism is wired end-to-end and where the main responsibilities live.

## 1) Runtime Architecture

```text
main.rs
  -> app::run()
      -> winit EventLoop
          -> App (ApplicationHandler)
              -> update simulation
              -> schedule/build chunk meshes
              -> upload meshes to GPU
              -> render world + overlays
```

```text
                    +-----------------------+
Input (winit) ----> | App (src/app.rs)      |
                    | - UI state/modes      |
                    | - camera mode          |
                    | - world edit actions   |
                    | - chunk visibility     |
                    +-----------+-----------+
                                |
            +-------------------+-------------------+
            |                                       |
            v                                       v
 +------------------------+               +------------------------+
 | Game Systems           |               | Render Systems         |
 | src/game/*             |               | src/render.rs          |
 | - World + Terrain      |               | - GpuState             |
 | - Physics              |               | - Chunk mesh buffers   |
 | - Inventory            |               | - camera/lighting UBOs |
 | - Interaction/Raycast  |               | - world + overlay pass |
 +-----------+------------+               +-----------+------------+
             |                                        ^
             +-------- chunk mesh vertices -----------+
```

## 2) Data Ownership and Flow

```text
World
  procedural terrain + mutable overrides + sub-block overrides
  (Arc<RwLock<HashMap<...>>> for edit layers)

App
  owns current World
  owns ActorRoster, Inventory, UI states, GraphicsSettings
  owns chunk residency + dirty/request/version tracking

ChunkBuildPipeline
  worker threads each clone World
  receives ChunkBuildRequest
  returns ChunkBuildResult(vertices)

GpuState
  receives vertices per ChunkRenderKey
  stores per-chunk GPU vertex buffers
  renders all resident chunk meshes each frame
```

## 3) Update/Render Tick

```text
about_to_wait():
  1. Frame cap timing
  2. dt + world_time_seconds update
  3. Sim update (player/free-cam)
  4. Recompute visible chunk set if chunk center changed
  5. Drain mesh worker results (upload capped per frame)
  6. Update lighting/env uniforms
  7. request_redraw()

RedrawRequested:
  1. Build gameplay/hud/menu/terrain-lab overlay vertices
  2. GpuState::render()
     - world pass (3D chunks)
     - overlay pass (2D UI)
```

## 4) World + Editing Model

```text
Raycast (interact.rs)
  -> hit cell / previous cell / face normal
  -> block edit:
       remove hit cell OR place at previous cell
  -> sub-block edit:
       map point -> subdivided slot (3x3 or 6x6)
       add/remove sub override entry

Any world edit
  -> mark_block_change_dirty(x, z)
  -> invalidate nearby LOD0 chunks + aligned higher LOD keys
  -> async remesh
```

## 5) Tooling Architecture

```text
terrain_tool (src/bin/terrain_tool.rs)
  - text description -> TerrainConfig tweaks
  - writes TerrainRecipe files
  - manages terrain/assets.registry

terrain_lab (src/bin/terrain_lab.rs)
  - interactive mode launches app terrain-lab mode
  - CLI mode samples region stats/ASCII/CSV and saves recipes

perf_suite (src/bin/perf_suite.rs)
  - executes chunk meshing scenarios by LOD mix
  - writes latest + history CSVs under perf/
```

## 6) Current Architectural Quirks

- `src/app.rs` is a large orchestration file containing UI, gameplay, chunk streaming, and edit logic in one place.
- `World` combines procedural generation, mutable edit state, meshing, and lighting prep.
- Asset registry + prefab catalog are implemented but not integrated into the runtime render pipeline/world placement loop.

See [loose_ends.md](loose_ends.md) for a concrete cleanup and integration plan.
