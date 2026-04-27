use std::{
    collections::VecDeque,
    net::{SocketAddr, UdpSocket},
    time::Duration,
};

use crate::{
    game::{physics::PhysicsConfig, world::World},
    server::session::ServerSession,
    shared::net::{
        protocol::{InputPacket, PlayerNetId, SnapshotPacket},
        wire::{
            ClientWireMessage, ServerWireMessage, decode_server_message, encode_client_message,
            hello_message,
        },
    },
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
    fn take_assigned_player_id(&mut self) -> Option<PlayerNetId>;
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
        StartupGameMode::MultiplayerDirect => Box::new(RemoteClient::new(
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

    fn take_assigned_player_id(&mut self) -> Option<PlayerNetId> {
        None
    }
}

struct RemoteClient {
    socket: UdpSocket,
    remote_addr: SocketAddr,
    initial_local_player_id: PlayerNetId,
    assigned_player_id: Option<PlayerNetId>,
    pending_assigned_player_id: Option<PlayerNetId>,
    outbound_inputs: VecDeque<InputPacket>,
    inbound_snapshots: VecDeque<SnapshotPacket>,
    hello_cooldown_remaining: Duration,
}

impl RemoteClient {
    fn new(local_player_id: PlayerNetId, remote_address: String) -> Self {
        let socket = UdpSocket::bind("0.0.0.0:0").expect("failed to bind local UDP client socket");
        socket
            .set_nonblocking(true)
            .expect("failed to set client UDP socket nonblocking");
        let remote_addr = remote_address
            .parse::<SocketAddr>()
            .expect("invalid remote server address");

        Self {
            socket,
            remote_addr,
            initial_local_player_id: local_player_id,
            assigned_player_id: None,
            pending_assigned_player_id: None,
            outbound_inputs: VecDeque::new(),
            inbound_snapshots: VecDeque::new(),
            hello_cooldown_remaining: Duration::ZERO,
        }
    }

    fn send_hello(&self) {
        let Ok(bytes) = encode_client_message(&hello_message()) else {
            return;
        };
        let _ = self.socket.send_to(&bytes, self.remote_addr);
    }

    fn send_input_packet(&self, mut packet: InputPacket, player_id: PlayerNetId) {
        for command in &mut packet.commands {
            command.player_id = player_id;
        }
        let Ok(bytes) = encode_client_message(&ClientWireMessage::Input(packet)) else {
            return;
        };
        let _ = self.socket.send_to(&bytes, self.remote_addr);
    }

    fn poll_inbound(&mut self) {
        let mut recv_buf = [0_u8; 64 * 1024];
        loop {
            match self.socket.recv_from(&mut recv_buf) {
                Ok((bytes_read, from_addr)) => {
                    if from_addr != self.remote_addr {
                        continue;
                    }
                    let Ok(message) = decode_server_message(&recv_buf[..bytes_read]) else {
                        continue;
                    };
                    match message {
                        ServerWireMessage::Snapshot {
                            assigned_player_id,
                            packet,
                        } => {
                            if self.assigned_player_id != Some(assigned_player_id) {
                                self.assigned_player_id = Some(assigned_player_id);
                                self.pending_assigned_player_id = Some(assigned_player_id);
                            }
                            self.inbound_snapshots.push_back(packet);
                        }
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
    }
}

impl ClientNetworkInterface for RemoteClient {
    fn mode(&self) -> StartupGameMode {
        StartupGameMode::MultiplayerDirect
    }

    fn enqueue_input_packet(&mut self, packet: InputPacket) {
        self.outbound_inputs.push_back(packet);
    }

    fn tick(&mut self, _world: &World, _physics: &PhysicsConfig, dt: f32) {
        self.hello_cooldown_remaining = self
            .hello_cooldown_remaining
            .saturating_sub(Duration::from_secs_f32(dt.max(0.0)));
        if self.assigned_player_id.is_none() && self.hello_cooldown_remaining.is_zero() {
            self.send_hello();
            self.hello_cooldown_remaining = Duration::from_millis(500);
        }

        let outbound_player_id = self
            .assigned_player_id
            .unwrap_or(self.initial_local_player_id);
        while let Some(packet) = self.outbound_inputs.pop_front() {
            self.send_input_packet(packet, outbound_player_id);
        }

        self.poll_inbound();
    }

    fn drain_snapshots(&mut self) -> Vec<SnapshotPacket> {
        self.inbound_snapshots.drain(..).collect()
    }

    fn take_assigned_player_id(&mut self) -> Option<PlayerNetId> {
        self.pending_assigned_player_id.take()
    }
}
