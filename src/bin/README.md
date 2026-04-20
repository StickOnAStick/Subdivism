# `src/bin/` Tooling Binaries

Subdivism includes four utility binaries.

## `terrain_tool`

Purpose:
- Generate terrain recipe files from profile + freeform description
- Manage texture/biome entries in `terrain/assets.registry`

Examples:
```bash
cargo run --bin terrain_tool -- --describe "alpine cliffs with soft valleys" --seed 424242 --out terrain/alpine.terrain
cargo run --bin terrain_tool -- --assets-list
```

## `terrain_lab`

Purpose:
- Interactive terrain tuning mode (in-game panel) when run without `--cli`
- Offline CLI sampling mode with ASCII/CSV summaries and recipe export

Examples:
```bash
cargo run --bin terrain_lab
cargo run --bin terrain_lab -- --cli --profile canyon --set biome_blend=0.2 --ascii
```

## `perf_suite`

Purpose:
- Benchmark chunk mesh throughput by scenario and LOD mix
- Benchmark world query throughput
- Write latest + historical metrics CSVs into `perf/`

Examples:
```bash
cargo run --bin perf_suite
cargo run --bin perf_suite -- --full
```

## `texture_lab`

Purpose:
- Interactive texture/material tuning mode with palette sliders and brush operations
- Save edited block styles to `terrain/block_styles.lab`
- Push the selected lab block directly into the active hotbar slot for in-world testing

Examples:
```bash
cargo run --bin texture_lab
cargo run -- --texture-lab
```

## Notes

- Tool binaries are currently implemented with direct `panic!/expect` paths for many failures.
- They are excellent for local iteration, but for CI/automation they should be migrated to `Result`-driven command exits.
