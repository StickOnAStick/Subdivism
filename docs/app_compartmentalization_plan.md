# App Compartmentalization Plan

## Why This Exists
`src/app.rs` currently owns too many responsibilities in one type:

- Platform lifecycle (`winit`, window creation, resize, redraw, event loop)
- GPU/render scheduling
- Camera/player movement and simulation update flow
- Chunk streaming orchestration and mesh worker flow
- UI/menu/inventory/terrain-lab interaction state
- Terrain/world replacement and preset management
- Block editing/subdivision logic
- Runtime timing/debug data and diagnostics

This makes it hard to reason about behavior, test in isolation, and safely change one area without risking another.

## Refactor Goals

- Reduce `App` field sprawl by introducing nested state objects with clear ownership.
- Keep behavior identical while we move data and logic.
- Make follow-up extraction into dedicated controllers straightforward.
- Preserve compile/run stability at every phase (`cargo check` after each phase).

## Target Shape

`App` should become a coordinator with owned subcomponents:

- `PlatformRuntimeState`: window id, window handle, gpu state.
- `CameraRuntimeState`: active camera mode, free camera, lens.
- `WorldSessionState`: seed/profile/dev/lab metadata.
- `RuntimeState`: frame scheduling, world time, RNG seed, shutdown flag.
- `BuildModeState`: subdivision/editing wallet and active edit scale.
- `DiagnosticsState`: debug overlay and chunk-worker diagnostics.
- Existing extracted states remain:
  - `MenuState`
  - `TerrainLabState`
  - `ChunkStreamingState`

## Phases

## Phase 0: Baseline Audit
- [x] Identify all `App` concerns and map to state buckets.
- [x] Confirm existing extracted state in `src/app/components.rs`.

## Phase 1: State Extraction (No Behavioral Change)
- [x] Add new grouped state structs in `src/app/components.rs`.
- [x] Replace corresponding flat `App` fields with nested state objects.
- [x] Update all callsites (`self.foo` -> `self.group.foo`) and keep behavior stable.

## Phase 2: Method Ownership Cleanup
- [ ] Introduce focused impl sections (or helper controllers) for:
  - camera/input flow
  - world/chunk streaming orchestration
  - menu/inventory/terrain-lab UI flow
  - block editing/subdivision flow
- [ ] Move pure helpers out of `App` where possible.

## Phase 3: Event-Path Decomposition
- [ ] Break `window_event` into smaller handlers:
  - keyboard handler
  - mouse handler
  - redraw pipeline
  - resize handling
- [ ] Break `about_to_wait` into frame-step pipeline helpers.

## Phase 4: Validation + Follow-up
- [x] `cargo check`
- [x] `cargo test`
- [ ] Capture residual god-object responsibilities and plan next split.

## Progress Log

- 2026-04-18: Created plan and concern map. Beginning Phase 1 extraction.
- 2026-04-18: Completed Phase 1 nested state extraction. `App` now uses grouped runtime/session objects (`platform`, `camera`, `session`, `runtime`, `diagnostics`, `build_mode`) and keeps existing behavior.
- 2026-04-18: Validation pass complete (`cargo check`, `cargo test`).
