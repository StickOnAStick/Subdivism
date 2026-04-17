# Loose Ends and Cleanup Plan

This is a concrete list of integration gaps, risky patterns, and cleanup targets found while iterating the repo.

## High Priority

1. Monolithic app orchestration (`src/app.rs`)
- Symptom: one file handles event loop, UI navigation, chunk streaming, gameplay input, inventory actions, terrain lab controls, and world edit logic.
- Risk: high change coupling, difficult testing, frequent regressions from unrelated edits.
- Fix path:
  - Split into modules: `app/input`, `app/ui`, `app/chunk_stream`, `app/editing`, `app/terrain_lab`.
  - Keep `App` as thin coordinator over subsystem structs.

2. World API is too broad (`src/game/world.rs`)
- Symptom: `World` owns procedural terrain, mutable overrides, meshing, lighting preprocessing, biome logic, and noise helpers.
- Risk: hard to isolate performance work, mesh changes can accidentally alter gameplay semantics.
- Fix path:
  - Extract `terrain_gen`, `voxel_storage`, `meshing`, `lighting` modules.
  - Keep `World` as facade with smaller internal components.

3. Sub-block placement can spend inventory before final set succeeds (`src/app.rs`, `edit_sub_block_from_click`)
- Symptom: `spend_sub_units(...)` happens before `set_sub_block_i64(...)`.
- Risk: edge-case material loss if placement fails after spending.
- Fix path:
  - Pre-validate fully, then spend and place in one transactional method.
  - If set fails, immediate refund fallback.

4. Hard process exit in pause menu (`std::process::exit(0)` in `src/app.rs`)
- Symptom: bypasses graceful shutdown path.
- Risk: future resources/telemetry/save hooks will not run.
- Fix path:
  - Route quit through event loop exit path (`event_loop.exit()` driven by app state flag).

## Medium Priority

1. Public API surface is wider than needed (`pub fn` across many internal helpers)
- Symptom: many functions are publicly visible despite being crate-internal implementation details.
- Risk: accidental coupling from tools/tests/bins, harder refactors.
- Fix path:
  - Narrow visibility to `pub(crate)` or private where cross-crate export is not required.
  - Keep explicit stable API list in `src/lib.rs`.

2. Terrain parameter metadata is duplicated
- Symptom: min/max/step/labels live in `terrain_params.rs`, while valid keys/clamping logic also exist in `TerrainConfig`.
- Risk: drift between UI controls and runtime validation.
- Fix path:
  - Create one parameter descriptor table (`name, min, max, step, getter, setter`), shared by lab UI and clamping/validation.

3. `sub_blocks_in_cell` is linear over all sub-overrides (`src/game/world.rs`)
- Symptom: per-cell lookup scans the entire `HashMap`.
- Risk: collision/raycast performance degrades with many sub-block edits.
- Fix path:
  - Add secondary index keyed by `(x, y, z)` to list sub entries for that cell.

4. Unused/disconnected systems
- `PrefabCatalog` exists but is not used in runtime world generation/edit cycle.
- `AssetRegistry` exists for textures/biomes but render path still uses procedural colors only.
- Fix path:
  - Decide: integrate into runtime (stream prefab spawns, texture material binding) or remove until needed.

5. Legacy artifact in repo (`backup/main_old.rs`)
- Symptom: old monolithic prototype remains in active tree.
- Risk: confusion for contributors and docs drift.
- Fix path:
  - Move to a clearly marked `archive/` directory or remove after tagging history.

## Low Priority

1. Error handling style in tooling binaries
- Symptom: frequent `panic!`/`expect` in `src/bin/*`.
- Risk: rough UX for CLI users; brittle automation.
- Fix path:
  - Return `Result` from command runners, print friendly errors, non-zero exit codes.

2. Stringly-typed UI/menu/keys
- Symptom: menu labels and key handling logic are mostly inline literals.
- Risk: translation/customization and consistency are hard.
- Fix path:
  - Use small command enum + label table for menu entries and keymaps.

3. `World::generate(_size_x, _size_y, _size_z)` ignores size args
- Symptom: API implies dimensioned worlds but implementation is infinite procedural surface.
- Risk: misleading API contract.
- Fix path:
  - Rename or deprecate size args; replace with `generate_default()` and explicit seeded/profile constructors.

4. `center_column()` currently always returns `(0, 0)`
- Symptom: suggests dynamic world center but is fixed.
- Risk: misleading semantics for spawn/randomization features.
- Fix path:
  - Either remove and inline `(0,0)`, or make center configurable.

## Integration/Removal Decision Matrix

1. Integrate now
- `AssetRegistry` into rendering material pipeline
- Prefab spawning into world/chunk generation loop
- Parameter descriptor unification for terrain lab + runtime validation

2. Keep but harden
- Async chunk build pipeline
- Sub-voxel editing wallet model
- Performance suite history pipeline

3. Remove or archive
- `backup/main_old.rs`
- Empty `skills/VOXEL_GAME_ENGINE.md` (either populate or remove)
- Any truly unused public helpers after visibility audit

## Suggested Execution Order

1. Extract `app.rs` into subsystems without behavior change.
2. Introduce parameter descriptor table and migrate terrain lab.
3. Make sub-block placement transactional and index sub-block lookups.
4. Decide and execute AssetRegistry/Prefab integration scope.
5. Shrink public API and archive/remove legacy artifacts.
