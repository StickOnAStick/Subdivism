use glam::{Quat, Vec3};

use crate::shared::net::protocol::{PlayerNetId, Tick, VehicleNetId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatRole {
    Pilot,
    Passenger,
}

#[derive(Clone, Copy, Debug)]
pub struct MountedState {
    pub vehicle_id: VehicleNetId,
    pub seat_index: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct SeatState {
    pub role: SeatRole,
    pub local_offset: Vec3,
    pub occupant: Option<PlayerNetId>,
}

#[derive(Clone, Copy, Debug)]
pub struct VehicleState {
    pub vehicle_id: VehicleNetId,
    pub position: Vec3,
    pub orientation: Quat,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
    pub throttle: f32,
}

impl Default for VehicleState {
    fn default() -> Self {
        Self {
            vehicle_id: 0,
            position: Vec3::ZERO,
            orientation: Quat::IDENTITY,
            linear_velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            throttle: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PlayerState {
    pub player_id: PlayerNetId,
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    pub mounted: Option<MountedState>,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            player_id: 0,
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            on_ground: false,
            mounted: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SimWorldState {
    pub tick: Tick,
    pub players: Vec<PlayerState>,
    pub vehicles: Vec<VehicleState>,
    pub vehicle_seats: Vec<Vec<SeatState>>,
}

impl SimWorldState {
    pub fn player(&self, player_id: PlayerNetId) -> Option<&PlayerState> {
        self.players
            .iter()
            .find(|player| player.player_id == player_id)
    }

    pub fn player_mut(&mut self, player_id: PlayerNetId) -> Option<&mut PlayerState> {
        self.players
            .iter_mut()
            .find(|player| player.player_id == player_id)
    }

    pub fn vehicle(&self, vehicle_id: VehicleNetId) -> Option<&VehicleState> {
        self.vehicles
            .iter()
            .find(|vehicle| vehicle.vehicle_id == vehicle_id)
    }

    pub fn vehicle_mut(&mut self, vehicle_id: VehicleNetId) -> Option<&mut VehicleState> {
        self.vehicles
            .iter_mut()
            .find(|vehicle| vehicle.vehicle_id == vehicle_id)
    }
}
