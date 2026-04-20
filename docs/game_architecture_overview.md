# Game Architecture Overview

This document focuses on the playable runtime (`cargo run`) and explains:
- the game entry point,
- subsystem responsibilities,
- what gets loaded,
- when it gets loaded,
- and why that load timing exists.

For broader design notes, also see [design_overview.md](design_overview.md).

## 1) Primary Entry Point

### Normal game launch

`src/main.rs` is the primary executable entry:

```text
main()
  -> subdivism::app::run()
      -> AppLaunchOptions::from_env()
      -> winit EventLoop::run_app(App)
```

### Terrain Lab interactive launch

`src/bin/terrain_lab.rs` launches the same runtime in a specialized mode:

```text
terrain_lab main()
  -> subdivism::app::run_terrain_lab()
      -> AppLaunchOptions::from_env()
      -> force terrain_lab = true
      -> winit EventLoop::run_app(App)
```

## 2) Startup Timeline (What Loads, When, Why)

### Stage A: Process start and launch options

When:
- immediately at process start, before `App` is created.

What loads:
- command-line arguments and selected environment flags (`--seed`, `--terrain-file`, `--dev`, `--terrain-lab`, `SUBDIVISM_TERRAIN_LAB`).

Why:
- these options define session mode and initial world source before any expensive initialization.

Notes:
- if no `--terrain-file` is supplied and `terrain/default.terrain` exists, it is used automatically.

### Stage B: `App::new(...)` bootstrap (CPU-side runtime state)

When:
- once, before the OS window/GPU exists.

What loads and initializes:
- persisted user runtime settings from `settings/user.settings` (if present).
  - contains render distance, frame cap index, and graphics settings.
  - values are clamped/sanitized before use.
- terrain recipe file (`--terrain-file` or default `terrain/default.terrain`) if present.
- `World` from `TerrainConfig + seed`.
  - important: this does not prebuild all chunk meshes. The world is largely procedural, with mutable overrides.
- gameplay/session runtime state:
  - `ActorRoster`, `CameraRuntimeState`, `MenuState`, `Inventory`, `PhysicsConfig`.
- chunk streaming pipeline:
  - `ChunkStreamingState` + `ChunkBuildPipeline`.
  - worker threads are spawned here and wait for mesh build jobs.
- diagnostics/runtime containers:
  - `RuntimeState`, `WorldSessionState`, `DiagnosticsState`, `BuildModeState`.

Why:
- this stage creates all non-window, non-GPU systems so app behavior can be orchestrated immediately once the platform is ready.

### Stage C: `resumed(...)` platform and GPU initialization

When:
- first `winit` resume callback, after event loop starts and a window can be created.

What loads and initializes:
- OS window (`1280x720` default).
- camera lens aspect and far plane tuned to current render distance.
- `GpuState`:
  - surface + adapter + device + queue,
  - swapchain config,
  - world and overlay pipelines,
  - camera/lighting/screen uniform buffers,
  - depth texture and overlay vertex buffer.
- initial chunk visibility scheduling from current camera chunk.

Why:
- window/surface/GPU resources depend on the platform event lifecycle and are intentionally delayed until `resumed`.

### Stage D: Frame loop (`about_to_wait` then `RedrawRequested`)

When:
- every frame after startup.

What runs in `about_to_wait`:
- frame pacing (`frame_cap_index`).
- `dt` update, world time advance, diagnostics frame record.
- simulation update (`update_player` or `update_free_camera`).
- chunk visibility scheduling if camera moved chunk.
- async mesh result drain and GPU upload (capped per frame).
- environment uniform update from time + graphics settings.
- `window.request_redraw()`.

What runs in `RedrawRequested`:
- overlay geometry assembly (HUD, debug overlay, menu, inventory, terrain-lab panel).
- `GpuState::render(...)`:
  - 3D world pass (resident chunk meshes),
  - 2D overlay pass.

Why:
- separation keeps simulation/scheduling in one phase and draw submission in the redraw phase expected by `winit`.

## 3) Subsystem Responsibilities

| Subsystem | Key Module(s) | Loaded / Activated | Intended Purpose |
|---|---|---|---|
| App Coordinator | `src/app.rs` | `App::new`, then all event callbacks | Central orchestration across input, simulation, streaming, UI, and rendering handoff. |
| Launch + Session Config | `AppLaunchOptions`, `WorldSessionState` | Process start + `App::new` | Resolve mode/seed/recipe path/dev flags that shape the runtime session. |
| Persisted Runtime Settings | `src/app/settings.rs` | Read in `App::new`, write on user changes | Preserve user graphics/runtime preferences between runs. |
| World Model + Terrain | `src/game/world/*` | Constructed in `App::new`; queried continuously | Provide procedural voxel world, mutable block/sub-block overrides, and chunk meshing functions. |
| Actor + Physics | `src/game/actor.rs`, `src/game/physics.rs` | `App::new` then per-frame updates | Player state, movement integration, collision, respawn behavior. |
| Chunk Streaming + Meshing | `ChunkStreamingState`, `ChunkBuildPipeline` | Workers spawned in `App::new`; scheduled per frame | Decide visible chunk LOD keys, request async mesh builds, track dirty/versioned meshes. |
| Rendering Backend | `src/render.rs` (`GpuState`) | Created in `resumed`; used every redraw | Own GPU resources and submit world + overlay draw passes. |
| Interaction / Editing | `src/game/interact.rs` + app edit logic | Input-driven during gameplay | Raycast targeting and block/sub-block placement/removal, plus dirty-chunk invalidation. |
| UI + Overlay | `debug_overlay.rs` + app menu/inventory builders | Built each redraw | Render HUD, diagnostics, pause/settings menus, inventory, terrain lab panel. |
| Terrain Lab Mode | `run_terrain_lab`, terrain-lab handlers in `app.rs` | Optional mode | In-runtime terrain parameter tuning and preset save workflow. |

## 4) Runtime Data Flow (Frame-Level)

```text
Input events (keyboard/mouse/wheel)
  -> App state mutations (menu, mode, edit requests, movement intent)
  -> update(dt)
      -> player/free-camera movement
      -> world edits mark chunk keys dirty
  -> chunk scheduler enqueues mesh jobs
  -> worker threads build vertices
  -> main thread uploads completed meshes to GpuState
  -> redraw
      -> world render pass
      -> overlay render pass
```

## 5) Disk-Backed Loads in the Main Runtime

| Path | When | Why |
|---|---|---|
| `settings/user.settings` | startup load; saved on graphics/runtime option changes | Keep render distance, frame cap, and graphics settings persistent. |
| `terrain/default.terrain` | startup if no explicit terrain file and file exists | Provide default world recipe for normal `cargo run` path. |
| `--terrain-file <path>` | startup (and reload via `F6` in dev mode) | Load chosen terrain profile/seed/params from disk. |
| `terrain/presets/*.terrain` | only when Terrain Lab save is invoked | Persist newly authored terrain presets. |

Additional note:
- `shader.wgsl` and `overlay.wgsl` are embedded at compile time via `include_str!`, so they are not runtime file loads.

## 6) Practical Mental Model

If you are tracing behavior:
1. Start at `src/main.rs` and `src/app.rs::run`.
2. Read `App::new` for startup decisions and persistent settings application.
3. Read `resumed` for platform/GPU bring-up.
4. Read `window_event` for input-driven state changes.
5. Read `about_to_wait` for per-frame simulation/streaming.
6. Read `RedrawRequested` path for final rendering composition.
