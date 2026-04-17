# `src/` Core Runtime and Rendering

This directory contains the main runtime loop and graphics layers.

## Modules

- `main.rs`
  - Binary entrypoint
  - Calls `subdivism::app::run()`

- `lib.rs`
  - Exposes crate modules (`app`, `camera`, `debug_overlay`, `game`, `mesh`, `render`)

- `app.rs`
  - Main application coordinator
  - Handles:
    - Winit event lifecycle
    - UI modes (playing/pause/inventory/terrain-lab panel)
    - Camera modes (player/free)
    - Chunk visibility, request/version tracking, async mesh uploads
    - Input-driven block and sub-block editing

- `render.rs`
  - `GpuState` setup and frame submission
  - World chunk vertex buffer management keyed by `ChunkRenderKey`
  - Overlay pass and environment uniforms (sun/moon/fog/sky)

- `camera.rs`
  - `Camera` + `CameraLens`
  - View/projection matrix composition and forward vector helpers

- `mesh.rs`
  - Shared mesh vertex format (`Vertex`)
  - Utility primitive generator (`push_quad`)

- `debug_overlay.rs`
  - 2D immediate-mode overlay primitives/text
  - FPS/memory overlay and menu layout rendering helpers

- `shader.wgsl`
  - Main world shading pass (ambient/sun/moon/fog/vibrance)

- `overlay.wgsl`
  - Overlay pass for UI geometry

## Runtime Flow Summary

```text
about_to_wait() -> simulation update -> chunk scheduling/results -> request_redraw()
RedrawRequested -> build overlay vertices -> gpu.render(world + overlay)
```

## Important Quirks

- `app.rs` is currently large and multi-responsibility by design (orchestration + feature logic).
- Chunk meshing is async, but upload is capped per frame (`MAX_CHUNK_UPLOADS_PER_FRAME`) to stabilize frame-time spikes.
