use crossbeam_channel::{Receiver, Sender, unbounded};

use crate::{game::world::World, mesh::Vertex, render::ChunkRenderKey};

#[derive(Clone, Copy)]
pub(super) struct ChunkBuildRequest {
    pub(super) key: ChunkRenderKey,
    pub(super) version: u64,
}

pub(super) struct ChunkBuildResult {
    pub(super) key: ChunkRenderKey,
    pub(super) version: u64,
    pub(super) vertices: Vec<Vertex>,
}

pub(super) struct ChunkBuildPipeline {
    pub(super) request_tx: Sender<ChunkBuildRequest>,
    pub(super) result_rx: Receiver<ChunkBuildResult>,
}

impl ChunkBuildPipeline {
    pub(super) fn new(world: World) -> Self {
        let worker_count = desired_chunk_worker_count();
        let (request_tx, request_rx) = unbounded::<ChunkBuildRequest>();
        let (result_tx, result_rx) = unbounded::<ChunkBuildResult>();

        for worker_index in 0..worker_count {
            let rx = request_rx.clone();
            let tx = result_tx.clone();
            let worker_world = world.clone();
            let thread_name = format!("chunk-mesh-worker-{worker_index}");
            std::thread::Builder::new()
                .name(thread_name)
                .spawn(move || {
                    loop {
                        let request = match rx.recv() {
                            Ok(request) => request,
                            Err(_) => break,
                        };
                        let vertices = worker_world
                            .build_chunk_mesh_lod(request.key.origin_chunk, request.key.lod_level);
                        if tx
                            .send(ChunkBuildResult {
                                key: request.key,
                                version: request.version,
                                vertices,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                })
                .expect("failed to spawn chunk mesh worker");
        }

        drop(result_tx);
        Self {
            request_tx,
            result_rx,
        }
    }
}

pub(super) fn desired_chunk_worker_count() -> usize {
    if let Some(override_count) = chunk_worker_override_from_env() {
        return override_count;
    }

    let available = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);

    if available <= 2 {
        available.max(1)
    } else {
        available.saturating_sub(1).clamp(2, 8)
    }
}

pub(super) fn chunk_worker_override_from_env() -> Option<usize> {
    std::env::var("SUBDIVISM_CHUNK_WORKERS")
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .map(|parsed| parsed.clamp(1, 12))
}
