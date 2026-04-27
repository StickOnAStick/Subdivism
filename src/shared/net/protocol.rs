use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

pub type Tick = u32;
pub type InputSeq = u32;
pub type PlayerNetId = u32;
pub type VehicleNetId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlTarget {
    OnFoot,
    VehiclePilot {
        vehicle_id: VehicleNetId,
        seat_index: u8,
    },
}

impl Default for ControlTarget {
    fn default() -> Self {
        Self::OnFoot
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct InputAxes {
    pub forward: i8,
    pub strafe: i8,
    pub throttle: i8,
    pub yaw: i8,
    pub pitch: i8,
    pub roll: i8,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct InputButtons {
    pub jump: bool,
    pub sprint: bool,
    pub brake: bool,
    pub interact: bool,
    pub primary_action: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct PlayerInputCmd {
    pub seq: InputSeq,
    pub tick: Tick,
    pub player_id: PlayerNetId,
    pub target: ControlTarget,
    pub axes: InputAxes,
    pub buttons: InputButtons,
    pub look_delta_yaw: i16,
    pub look_delta_pitch: i16,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InputPacket {
    pub newest_seq: InputSeq,
    pub commands: Vec<PlayerInputCmd>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ActorSnapshot {
    pub player_id: PlayerNetId,
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub mounted_vehicle: Option<VehicleNetId>,
    pub mounted_seat: Option<u8>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct VehicleSnapshot {
    pub vehicle_id: VehicleNetId,
    pub position: Vec3,
    pub orientation: Quat,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
    pub throttle: f32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SnapshotPacket {
    pub server_tick: Tick,
    pub ack_input_seq: InputSeq,
    pub actors: Vec<ActorSnapshot>,
    pub vehicles: Vec<VehicleSnapshot>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum SeatRequestKind {
    Enter {
        vehicle_id: VehicleNetId,
        seat_index: u8,
    },
    Exit,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SeatRequest {
    pub player_id: PlayerNetId,
    pub tick: Tick,
    pub kind: SeatRequestKind,
}
