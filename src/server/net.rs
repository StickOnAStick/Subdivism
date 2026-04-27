use std::{
    collections::HashMap,
    net::{SocketAddr, UdpSocket},
    thread,
    time::{Duration, Instant},
};

use crate::{
    game::{physics::PhysicsConfig, world::World},
    server::session::ServerSession,
    shared::{
        net::wire::{
            ClientWireMessage, ServerWireMessage, decode_client_message, encode_server_message,
        },
        sim::step,
    },
};

pub fn run_udp_server(bind_addr: &str) {
    let socket = UdpSocket::bind(bind_addr)
        .unwrap_or_else(|err| panic!("failed to bind UDP server on {bind_addr}: {err}"));
    socket
        .set_nonblocking(true)
        .expect("failed to set UDP server socket nonblocking");

    let world = World::generate_default();
    let physics = PhysicsConfig::default();
    let mut session = ServerSession::new();

    let tick_hz = step::DEFAULT_SIM_TICK_HZ;
    let tick_dt = Duration::from_secs_f32(step::fixed_dt_seconds(tick_hz));
    let mut next_tick = Instant::now();

    let mut next_player_id = 0_u32;
    let mut clients: HashMap<SocketAddr, u32> = HashMap::new();
    let mut recv_buf = [0_u8; 64 * 1024];

    println!("subdivism UDP server listening on {bind_addr} @ {tick_hz}hz");

    loop {
        loop {
            match socket.recv_from(&mut recv_buf) {
                Ok((bytes_read, addr)) => {
                    let Ok(message) = decode_client_message(&recv_buf[..bytes_read]) else {
                        continue;
                    };
                    match message {
                        ClientWireMessage::Hello { .. } => {
                            clients.entry(addr).or_insert_with(|| {
                                let assigned = next_player_id;
                                next_player_id = next_player_id.wrapping_add(1);
                                assigned
                            });
                        }
                        ClientWireMessage::Input(packet) => {
                            let Some(player_id) = clients.get(&addr).copied() else {
                                continue;
                            };
                            session.ingest_input_packet(player_id, packet);
                        }
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }

        let now = Instant::now();
        if now >= next_tick {
            session.tick(&world, &physics, tick_dt.as_secs_f32());
            next_tick += tick_dt;

            for (addr, player_id) in &clients {
                let Some(snapshot) = session.latest_snapshot(*player_id) else {
                    continue;
                };
                let message = ServerWireMessage::Snapshot {
                    assigned_player_id: *player_id,
                    packet: snapshot.clone(),
                };
                let Ok(bytes) = encode_server_message(&message) else {
                    continue;
                };
                let _ = socket.send_to(&bytes, addr);
            }
        } else {
            thread::sleep(Duration::from_millis(1));
        }
    }
}
