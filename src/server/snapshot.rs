use crate::shared::{
    net::protocol::{ActorSnapshot, SnapshotPacket, VehicleSnapshot},
    sim::state::SimWorldState,
};

pub fn build_snapshot_packet(world: &SimWorldState, ack_input_seq: u32) -> SnapshotPacket {
    let actors = world
        .players
        .iter()
        .map(|player| ActorSnapshot {
            player_id: player.player_id,
            position: player.position,
            velocity: player.velocity,
            yaw: player.yaw,
            pitch: player.pitch,
            mounted_vehicle: player.mounted.map(|mounted| mounted.vehicle_id),
            mounted_seat: player.mounted.map(|mounted| mounted.seat_index),
        })
        .collect();

    let vehicles = world
        .vehicles
        .iter()
        .map(|vehicle| VehicleSnapshot {
            vehicle_id: vehicle.vehicle_id,
            position: vehicle.position,
            orientation: vehicle.orientation,
            linear_velocity: vehicle.linear_velocity,
            angular_velocity: vehicle.angular_velocity,
            throttle: vehicle.throttle,
        })
        .collect();

    SnapshotPacket {
        server_tick: world.tick,
        ack_input_seq,
        actors,
        vehicles,
    }
}
