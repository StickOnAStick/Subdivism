# App Module Boundaries

`App` should be a runtime coordinator, not a feature implementation container.

## Target Shape

`src/app.rs` keeps:
- App lifecycle wiring (`run`, `run_terrain_lab`, `ApplicationHandler` entrypoints)
- cross-component orchestration and state transitions
- shared primitives used by multiple components

Everything else should live in focused submodules.

## Responsibility Audit

### Moved in this refactor

- `app/perf.rs`
  - startup performance suite scenario generation
  - startup perf graph rendering
  - perf report data model

- `app/terrain_lab.rs`
  - terrain lab panel content
  - terrain lab input handling
  - terrain parameter tuning and preset save flow
  - terrain lab save-name utilities

### Still in `app.rs` (should be split)

- Startup/pause/inventory menu behavior and overlays
  - target: `app/menu.rs`
- HUD + gameplay overlay composition (hotbar, held item, subdivision, sky body)
  - target: `app/hud.rs`
- Build/edit interaction (ray hits, block/sub-block edits, subdivision wallet)
  - target: `app/build_mode.rs`
- Chunk streaming scheduler / dirty propagation / upload intake
  - target: `app/chunk_stream.rs`
- Camera/player update loop and movement integration
  - target: `app/movement.rs`
- App bootstrap and world replacement helpers
  - target: `app/bootstrap.rs`
- Shared UI helpers (`slider_*`, sprite helpers)
  - target: `app/ui_primitives.rs`

## Refactor Strategy

1. Extract by feature seam, not by random function size.
2. Keep behavior identical in each extraction step.
3. Ensure every extraction compiles and passes `cargo test --lib`.
4. Only after seams are stable, tighten visibility and reduce `App` field access.

## Proposed Incremental Plan

1. Extract `menu.rs` (startup/pause/inventory input + menu overlays).
2. Extract `hud.rs` (hud lines + all overlay builders).
3. Extract `build_mode.rs` (click editing + sub-block wallet/scale).
4. Extract `chunk_stream.rs` (request scheduling + dirty propagation).
5. Extract `movement.rs` (player/free-cam update).
6. Extract shared constants/helpers into `ui_primitives.rs` and `bootstrap.rs`.

## Guardrails

- No gameplay behavior changes during extraction.
- No changes to bin entry behavior.
- Keep `App` as state owner until after module seams settle.
