use std::collections::HashMap;

use glam::Vec3;

use crate::{
    game::{physics::PhysicsConfig, world::World},
    server::{authority::AuthorityState, snapshot},
    shared::net::protocol::{InputPacket, PlayerNetId, SnapshotPacket},
};

#[derive(Debug)]
pub struct ClientSessionState {
    pub player_id: PlayerNetId,
    pub last_acknowledged_input_seq: u32,
}

#[derive(Debug)]
pub struct ServerSession {
    authority: AuthorityState,
    clients: HashMap<PlayerNetId, ClientSessionState>,
    cached_snapshots: HashMap<PlayerNetId, SnapshotPacket>,
}

impl ServerSession {
    pub fn new() -> Self {
        let mut authority = AuthorityState::new();
        authority.ensure_player(0, Vec3::new(0.0, 96.0, 0.0));
        Self {
            authority,
            clients: HashMap::new(),
            cached_snapshots: HashMap::new(),
        }
    }

    pub fn ingest_input_packet(&mut self, player_id: PlayerNetId, packet: InputPacket) {
        let client = self.clients.entry(player_id).or_insert(ClientSessionState {
            player_id,
            last_acknowledged_input_seq: 0,
        });

        for cmd in packet.commands {
            self.authority.enqueue_input(cmd);
        }
        client.last_acknowledged_input_seq =
            client.last_acknowledged_input_seq.max(packet.newest_seq);
    }

    pub fn tick(&mut self, world: &World, physics: &PhysicsConfig, dt: f32) {
        self.authority.step(world, physics, dt);

        for (player_id, client) in &self.clients {
            let snapshot = snapshot::build_snapshot_packet(
                &self.authority.world,
                client.last_acknowledged_input_seq,
            );
            self.cached_snapshots.insert(*player_id, snapshot);
        }

        if self.clients.is_empty() {
            let snapshot = snapshot::build_snapshot_packet(&self.authority.world, 0);
            self.cached_snapshots.insert(0, snapshot);
        }
    }

    pub fn latest_snapshot(&self, player_id: PlayerNetId) -> Option<&SnapshotPacket> {
        self.cached_snapshots.get(&player_id)
    }
}
