use glam::Vec3;

use crate::{
    camera::{Camera, CameraLens},
    game::actor::{ActorState, PLAYER_EYE_HEIGHT},
    shared::{net::protocol::VehicleNetId, sim::state::SimWorldState},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CameraTarget {
    Free,
    LocalPlayerEye,
    VehicleSeatFirstPerson {
        vehicle_id: VehicleNetId,
        seat_index: u8,
    },
    VehicleChase {
        vehicle_id: VehicleNetId,
        distance: f32,
        height: f32,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct CameraRig {
    pub target: CameraTarget,
    pub yaw: f32,
    pub pitch: f32,
}

impl CameraRig {
    pub fn from_local_player(player: &ActorState) -> Self {
        Self {
            target: CameraTarget::LocalPlayerEye,
            yaw: player.look.yaw,
            pitch: player.look.pitch,
        }
    }

    pub fn resolve_basic(
        &self,
        local_player: &ActorState,
        free_camera: Camera,
        lens: CameraLens,
    ) -> Camera {
        match self.target {
            CameraTarget::Free => Camera {
                lens,
                ..free_camera
            },
            CameraTarget::LocalPlayerEye
            | CameraTarget::VehicleSeatFirstPerson { .. }
            | CameraTarget::VehicleChase { .. } => Camera {
                lens,
                ..local_player.camera(lens)
            },
        }
    }

    pub fn resolve_with_sim(
        &self,
        sim: &SimWorldState,
        local_player: &ActorState,
        free_camera: Camera,
        lens: CameraLens,
    ) -> Camera {
        match self.target {
            CameraTarget::Free | CameraTarget::LocalPlayerEye => {
                self.resolve_basic(local_player, free_camera, lens)
            }
            CameraTarget::VehicleSeatFirstPerson {
                vehicle_id,
                seat_index,
            } => {
                if let Some(vehicle) = sim.vehicle(vehicle_id) {
                    let seat_offset = Vec3::new(0.0, 1.4 + seat_index as f32 * 0.03, 0.0);
                    let position = vehicle.position + vehicle.orientation * seat_offset;
                    Camera {
                        position,
                        yaw: self.yaw,
                        pitch: self.pitch,
                        lens,
                    }
                } else {
                    self.resolve_basic(local_player, free_camera, lens)
                }
            }
            CameraTarget::VehicleChase {
                vehicle_id,
                distance,
                height,
            } => {
                if let Some(vehicle) = sim.vehicle(vehicle_id) {
                    let forward = vehicle.orientation * Vec3::X;
                    let position = vehicle.position - forward * distance + Vec3::Y * height;
                    Camera {
                        position,
                        yaw: self.yaw,
                        pitch: self.pitch,
                        lens,
                    }
                } else {
                    self.resolve_basic(local_player, free_camera, lens)
                }
            }
        }
    }

    pub fn follow_local_player(&mut self, player: &ActorState) {
        self.yaw = player.look.yaw;
        self.pitch = player.look.pitch;
    }

    pub fn clamp_pitch(&mut self, max_pitch: f32) {
        self.pitch = self.pitch.clamp(-max_pitch, max_pitch);
    }

    pub fn with_target(mut self, target: CameraTarget) -> Self {
        self.target = target;
        self
    }

    pub fn free_camera_from_rig(&self, lens: CameraLens, position: Vec3) -> Camera {
        Camera {
            position,
            yaw: self.yaw,
            pitch: self.pitch,
            lens,
        }
    }

    pub fn local_eye_position(local_player: &ActorState) -> Vec3 {
        local_player.motion.position + Vec3::Y * PLAYER_EYE_HEIGHT
    }
}
