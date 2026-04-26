# Core Module Interaction Map (ASCII)

This is a quick visual map of the main runtime modules and how data flows between them.

## 1) Runtime Core (Boxes + Arrows)

```text
                              +--------------------------------------+
                              |            Input / OS Events         |
                              |      winit::WindowEvent + DeviceEvent|
                              +-------------------+------------------+
                                                  |
                                                  v
+--------------------------+        +-------------+---------------+
| src/main.rs              | -----> | src/app.rs (App coordinator)|
| main() -> app::run()     |        | - lifecycle + game loop     |
+--------------------------+        | - chunk scheduling           |
                                    | - UI mode + input routing    |
                                    +------+------+----------+-----+
                                           |      |          |
                     uses camera math -----+      |          +---- builds overlay vertices
                                           |      |
                                           v      v
                              +------------+--+   +-------------------------+
                              | src/camera.rs |   | src/game/*              |
                              | Camera/Lens   |   | gameplay + world systems|
                              +-------+-------+   +-----------+-------------+
                                      ^                       |
                                      |                       | chunk mesh jobs
                                      |                       v
                                      |            +----------+------------------+
                                      |            | ChunkBuildPipeline (app.rs) |
                                      |            | worker threads clone World   |
                                      |            +----------+------------------+
                                      |                       |
                                      |                       v  Vec<Vertex>
                                      |            +----------+------------------+
                                      |            | src/mesh.rs                 |
                                      |            | Vertex + push_quad()         |
                                      |            +----------+------------------+
                                      |                       |
                                      +-----------------------+ upload
                                                              v
                                                    +---------+----------------+
                                                    | src/render.rs            |
                                                    | GpuState + chunk buffers |
                                                    | world pass + overlay pass|
                                                    +----+-----------------+---+
                                                         |                 |
                                         shader source --+                 +-- overlay vertices
                                                         |                 |
                                      +------------------v---+   +---------v------------------+
                                      | src/shader.wgsl      |   | src/debug_overlay.rs       |
                                      | world shading pass   |   | DebugOverlay + OverlayVertex|
                                      +----------------------+   +-----------------------------+
```

## 2) Gameplay Module Relationships (`src/game/*`)

```text
+---------------------------+
| game/world/mod.rs (World) |
| terrain + edits + meshing |
+-------------+-------------+
              |
              +--> world/storage.rs   (block + sub-block override state)
              +--> world/terrain_gen.rs
              |      +--> world/noise.rs     (noise + fbm helpers)
              |      +--> world/palette.rs   (biome/block color palette)
              +--> world/meshing.rs
                     +--> world/lighting.rs  (light volume + shading)
                     +--> mesh::push_quad()

+-------------------+      +------------------+
| game/actor.rs     | ----> | camera.rs        |
| actor state       |       | player camera    |
+---------+---------+       +------------------+
          ^
          | updates motion/collision against
+---------+---------+
| game/physics.rs   | -------------------------> game/world/mod.rs
+-------------------+

+-------------------+
| game/interact.rs  | ---> game/world/mod.rs   (raycast, placement targets)
|                   | ---> camera.rs            (projection helpers)
|                   | ---> debug_overlay.rs     (overlay lines)
+-------------------+

+-------------------+      +-------------------+
| game/inventory.rs | ---> | game/hud.rs       |
| slot model        |      | label/row format  |
+-------------------+      +-------------------+

+----------------------+     +-----------------------+
| game/terrain_params.rs| <-- | app/terrain_lab.rs   |
| TerrainConfig metadata|     | parameter tuning UI  |
+----------------------+     +-----------------------+

+----------------------+     +-----------------------+
| game/terrain_recipe.rs| <-- | app.rs + terrain_lab |
| load/save .terrain    |     | startup + preset save|
+----------------------+     +-----------------------+

+----------------------+     +-----------------------+
| game/asset_registry.rs| <-- | src/bin/terrain_tool |
| texture/biome registry|     | (tooling path)       |
+----------------------+     +-----------------------+

+----------------------+     +-----------------------+
| game/prefab.rs       |     | currently decoupled   |
| prefab catalog logic |     | from runtime world gen|
+----------------------+     +-----------------------+
```

## 3) Main Frame Loop Data Flow

```text
about_to_wait (App)
  -> physics/camera update
  -> world edits (if any)
  -> chunk requests -> worker mesh builds
  -> drain mesh results -> gpu.upsert_chunk_mesh(...)
  -> request_redraw()

RedrawRequested (App)
  -> build gameplay/menu/debug overlay vertices
  -> gpu.render(world chunks + overlays)
```
