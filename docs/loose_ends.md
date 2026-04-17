# Loose Ends and Maintainability Audit

This document replaces the previous loose-ends list and reflects the current state after the latest cleanup pass.

## What Was Fixed Already

1. Terrain parameter metadata is now centralized through `TerrainParamSpec` in `src/game/world.rs`.
2. Sub-block storage now has a cell index (`SubOverrideState`) instead of full-map scans for cell lookups.
3. Sub-block placement is now transactional in `src/app.rs` and avoids silent material loss.
4. Pause-menu quit now exits via the event loop instead of direct hard process exit.
5. `terrain_tool` and `terrain_lab` now use `Result`-style CLI error handling instead of panic-first flow.

## Current High-Risk Structure Debt

1. `src/app.rs` remains a monolith
- Symptoms: one file still owns input handling, UI routing, camera switching, chunk streaming, world edit rules, and overlay assembly.
- Why this is risky: unrelated changes collide in the same module; hard to test in isolation; fragile for contributors.
- Required fix: split into subsystem modules with explicit ownership boundaries.
- Suggested split:
`app/input.rs`, `app/ui.rs`, `app/editing.rs`, `app/chunk_streaming.rs`, `app/terrain_lab.rs`, `app/runtime.rs`.

2. `src/game/world.rs` is still a god-object
- Symptoms: terrain generation math, biome logic, mutable storage, mesh generation, and lighting prep remain mixed.
- Why this is risky: perf work and gameplay work cannot evolve independently; model and rendering concerns are interleaved.
- Required fix: isolate into separate components and keep `World` as facade.
- Suggested split:
`world/storage.rs`, `world/terrain_gen.rs`, `world/meshing.rs`, `world/lighting.rs`, `world/biomes.rs`.

3. Runtime systems are tightly coupled to immediate-mode UI text
- Symptoms: `app.rs` builds gameplay and menu text inline and drives behavior from positional menu indices.
- Why this is risky: high regression risk when changing menu layouts; poor readability; poor localization/extensibility.
- Required fix: move to command-based menu definitions and dedicated view-model builders.

4. Render layer accepts raw gameplay-side geometry payloads
- Symptoms: `GpuState::render` receives ad-hoc overlay vertex arrays assembled in app logic.
- Why this is risky: rendering API contract is wide and unstable; app and renderer are tightly bound.
- Required fix: define structured render commands or stable render packets per pass.

## Public API Surface Debt (Too Broad / Poorly Structured)

The crate currently exposes a large number of `pub fn` APIs that are implementation detail rather than stable boundaries.

### Public APIs likely to demote to `pub(crate)` or private

1. `src/debug_overlay.rs`
- Candidates: `add_quad`, `add_text`, `menu_layout`, `build_menu_vertices`, `build_vertices`.
- Reason: these are internal rendering helpers consumed only by app/runtime internals.

2. `src/mesh.rs`
- Candidate: `push_quad`.
- Reason: internal meshing primitive; not a stable external API.

3. `src/game/interact.rs`
- Candidates: most free helper functions (`direction_to_screen`, `face_plane_points`, `push_screen_line`, etc).
- Reason: currently app-internal interaction math; should be grouped behind a smaller service API.

4. `src/game/hud.rs`
- Candidates: formatter helpers.
- Reason: UI formatting internals, not domain API.

5. `src/game/actor.rs`
- Candidates: broad accessors/mutators on `ActorRoster` and `ActorState`.
- Reason: direct mutation patterns make future authority/network models harder.

6. `src/game/terrain_params.rs`
- All functions are wrappers around `TerrainConfig` descriptor access.
- Reason: this module is now mostly pass-through and can be folded into one canonical API.

7. `src/game/world.rs`
- Candidates for narrowing: many meshing/storage mutators and helpers that should be behind focused traits/components.
- Reason: `World` is currently used as both high-level facade and low-level toolbox.

## Integration Gaps Still Open

1. `AssetRegistry` is still disconnected from runtime rendering
- Current state: tooling can edit registry; game render path still uses procedural color palette only.
- Decision needed: integrate real material/texturing path or remove/defer registry from mainline workflow.

2. `PrefabCatalog` is still disconnected from world generation
- Current state: archetypes and spawn logic exist but are not integrated into chunk/world generation.
- Decision needed: wire spawn generation in runtime or archive until ready.

3. Tooling error handling still inconsistent
- `terrain_tool` and `terrain_lab` were cleaned up.
- `perf_suite` still has extensive `expect` calls and panic-style failure behavior.

4. Legacy/low-signal artifacts still present
- `backup/main_old.rs` should be archived or removed.
- `skills/VOXEL_GAME_ENGINE.md` is empty and should be populated or deleted.

## Additional Maintainability Smells

1. Duplicate slider/rendering constants and magic numbers spread through app and render paths.
2. Behavior encoded by menu index position rather than explicit command enum.
3. Mixed responsibilities in input event handlers (state transitions, simulation actions, and UI mutations in same blocks).
4. Limited boundaries for deterministic testing around app-level behavior orchestration.

## Refactor Plan (Updated)

1. Extract `app.rs` into ownership-based subsystem modules with no behavior change.
2. Split `world.rs` into storage, generation, meshing, and lighting submodules.
3. Narrow public API to an intentional crate facade (`lib.rs`) and demote internal helpers.
4. Resolve integration decision for `AssetRegistry` and `PrefabCatalog` (integrate or archive).
5. Finish CLI/runtime error-handling hardening (`perf_suite`, selected `expect` chains).
6. Remove or archive legacy artifacts.

## Definition of Done for This Audit

1. No monolithic orchestration files over mixed domains.
2. Public APIs are intentional and documented by module purpose.
3. Runtime-only helper APIs are not publicly exported by default.
4. Feature-adjacent systems are either integrated end-to-end or clearly marked experimental/archived.
