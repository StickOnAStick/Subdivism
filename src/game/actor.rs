use glam::Vec3;

use crate::camera::{Camera, CameraLens};

pub const PLAYER_EYE_HEIGHT: f32 = 1.62;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorId(pub usize);

/// Compact motion state that is easy to copy, snapshot, or replicate to remote peers.
#[derive(Clone, Copy, Debug, Default)]
pub struct MotionState {
    pub position: Vec3,
    pub velocity: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub struct LookState {
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct ActorState {
    pub motion: MotionState,
    pub look: LookState,
    pub on_ground: bool,
}

impl ActorState {
    pub fn player(_id: ActorId, spawn_position: Vec3) -> Self {
        Self {
            motion: MotionState {
                position: spawn_position,
                velocity: Vec3::ZERO,
            },
            look: LookState {
                yaw: -std::f32::consts::FRAC_PI_2,
                pitch: -0.35,
            },
            on_ground: false,
        }
    }

    pub fn camera(&self, lens: CameraLens) -> Camera {
        Camera {
            position: self.motion.position + Vec3::Y * PLAYER_EYE_HEIGHT,
            yaw: self.look.yaw,
            pitch: self.look.pitch,
            lens,
        }
    }

    pub fn forward_flat(&self) -> Vec3 {
        Vec3::new(self.look.yaw.cos(), 0.0, self.look.yaw.sin()).normalize_or_zero()
    }
}

/// Actor storage is kept in a contiguous `Vec` so local simulation and future replication stay cache-friendly.
pub struct ActorRoster {
    actors: Vec<ActorState>,
    local_player: ActorId,
}

impl ActorRoster {
    pub fn new(local_player_spawn: Vec3) -> Self {
        let local_player = ActorId(0);
        let actors = vec![ActorState::player(local_player, local_player_spawn)];
        Self {
            actors,
            local_player,
        }
    }

    pub fn local_player(&self) -> &ActorState {
        &self.actors[self.local_player.0]
    }

    pub fn local_player_mut(&mut self) -> &mut ActorState {
        &mut self.actors[self.local_player.0]
    }

    pub fn respawn_local_player(&mut self, spawn_position: Vec3) {
        let actor = self.local_player_mut();
        actor.motion.position = spawn_position;
        actor.motion.velocity = Vec3::ZERO;
        actor.on_ground = true;
    }
}
