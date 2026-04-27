use std::collections::VecDeque;

use crate::{
    game::{physics::PhysicsConfig, world::World},
    server::session::ServerSession,
    shared::net::protocol::{InputPacket, PlayerNetId, SnapshotPacket},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupGameMode {
    SinglePlayer,
    HostOpenServer,
    MultiplayerDirect,
}

impl StartupGameMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::SinglePlayer => "SOLO",
            Self::HostOpenServer => "HOST",
            Self::MultiplayerDirect => "JOIN",
        }
    }

    pub fn menu_label(self) -> &'static str {
        match self {
            Self::SinglePlayer => "SOLO (LOCAL AUTHORITY)",
            Self::HostOpenServer => "HOST (OPEN SERVER)",
            Self::MultiplayerDirect => "JOIN SERVER (DIRECT)",
        }
    }

    pub fn network_label(self) -> &'static str {
        match self {
            Self::SinglePlayer => "singleplayer-local",
            Self::HostOpenServer => "host-open-server",
            Self::MultiplayerDirect => "multiplayer-direct",
        }
    }
}

pub trait ClientNetworkInterface {
    fn mode(&self) -> StartupGameMode;
    fn enqueue_input_packet(&mut self, packet: InputPacket);
    fn tick(&mut self, world: &World, physics: &PhysicsConfig, dt: f32);
    fn drain_snapshots(&mut self) -> Vec<SnapshotPacket>;
}

pub fn build_client_network(
    mode: StartupGameMode,
    local_player_id: PlayerNetId,
) -> Box<dyn ClientNetworkInterface> {
    match mode {
        StartupGameMode::SinglePlayer => {
            Box::new(InProcessAuthorityClient::new(mode, local_player_id, false))
        }
        StartupGameMode::HostOpenServer => {
            Box::new(InProcessAuthorityClient::new(mode, local_player_id, true))
        }
        StartupGameMode::MultiplayerDirect => Box::new(RemoteClientStub::new(
            local_player_id,
            "127.0.0.1:4000".to_string(),
        )),
    }
}

struct InProcessAuthorityClient {
    mode: StartupGameMode,
    local_player_id: PlayerNetId,
    #[allow(dead_code)]
    host_open: bool,
    session: ServerSession,
    inbound_snapshots: VecDeque<SnapshotPacket>,
}

impl InProcessAuthorityClient {
    fn new(mode: StartupGameMode, local_player_id: PlayerNetId, host_open: bool) -> Self {
        Self {
            mode,
            local_player_id,
            host_open,
            session: ServerSession::new(),
            inbound_snapshots: VecDeque::new(),
        }
    }
}

impl ClientNetworkInterface for InProcessAuthorityClient {
    fn mode(&self) -> StartupGameMode {
        self.mode
    }

    fn enqueue_input_packet(&mut self, packet: InputPacket) {
        self.session
            .ingest_input_packet(self.local_player_id, packet);
    }

    fn tick(&mut self, world: &World, physics: &PhysicsConfig, dt: f32) {
        self.session.tick(world, physics, dt);
        if let Some(snapshot) = self.session.latest_snapshot(self.local_player_id) {
            self.inbound_snapshots.push_back(snapshot.clone());
        }
    }

    fn drain_snapshots(&mut self) -> Vec<SnapshotPacket> {
        self.inbound_snapshots.drain(..).collect()
    }
}

struct RemoteClientStub {
    local_player_id: PlayerNetId,
    remote_address: String,
    dropped_packet_count: u64,
}

impl RemoteClientStub {
    fn new(local_player_id: PlayerNetId, remote_address: String) -> Self {
        Self {
            local_player_id,
            remote_address,
            dropped_packet_count: 0,
        }
    }
}

impl ClientNetworkInterface for RemoteClientStub {
    fn mode(&self) -> StartupGameMode {
        StartupGameMode::MultiplayerDirect
    }

    fn enqueue_input_packet(&mut self, _packet: InputPacket) {
        let _ = self.local_player_id;
        let _ = &self.remote_address;
        self.dropped_packet_count = self.dropped_packet_count.saturating_add(1);
    }

    fn tick(&mut self, _world: &World, _physics: &PhysicsConfig, _dt: f32) {}

    fn drain_snapshots(&mut self) -> Vec<SnapshotPacket> {
        Vec::new()
    }
}
