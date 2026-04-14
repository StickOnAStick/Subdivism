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
