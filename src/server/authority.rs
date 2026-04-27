use std::collections::{HashMap, VecDeque};

use crate::{
    game::{actor::ActorState, physics::PhysicsConfig, world::World},
    shared::{
        net::protocol::{ControlTarget, PlayerInputCmd, PlayerNetId, Tick},
        sim::{
            state::{PlayerState, SimWorldState, VehicleState},
            step,
        },
    },
};

#[derive(Debug, Default)]
pub struct AuthorityState {
    pub tick: Tick,
    pub world: SimWorldState,
    input_queues: HashMap<PlayerNetId, VecDeque<PlayerInputCmd>>,
}

impl AuthorityState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue_input(&mut self, cmd: PlayerInputCmd) {
        self.input_queues
            .entry(cmd.player_id)
            .or_default()
            .push_back(cmd);
    }

    pub fn step(&mut self, terrain_world: &World, physics: &PhysicsConfig, dt: f32) {
        self.tick = self.tick.wrapping_add(1);
        self.world.tick = self.tick;
        let collision_view = terrain_world.collision_view();

        let player_ids: Vec<PlayerNetId> = self
            .world
            .players
            .iter()
            .map(|player| player.player_id)
            .collect();
        for player_id in player_ids {
            let mut consumed_command = false;
            if let Some(queue) = self.input_queues.get_mut(&player_id) {
                while let Some(cmd) = queue.pop_front() {
                    consumed_command = true;
                    match cmd.target {
                        ControlTarget::OnFoot => {
                            if let Some(player) = self.world.player_mut(player_id) {
                                let mut actor = player_to_actor(*player);
                                step::apply_player_input_cmd(
                                    &mut actor,
                                    &cmd,
                                    &collision_view,
                                    physics,
                                    dt,
                                );
                                *player = actor_to_player(player.player_id, actor, player.mounted);
                            }
                        }
                        ControlTarget::VehiclePilot { vehicle_id, .. } => {
                            if let Some(vehicle) = self.world.vehicle_mut(vehicle_id) {
                                step::apply_vehicle_pilot_input(vehicle, &cmd, dt);
                            }
                        }
                    }
                }
            }
            if consumed_command {
                continue;
            }
            // Keep authority-side physics running even when no new input arrives.
            let idle_cmd = PlayerInputCmd {
                tick: self.tick,
                player_id,
                target: ControlTarget::OnFoot,
                ..PlayerInputCmd::default()
            };
            if let Some(player) = self.world.player_mut(player_id) {
                let mut actor = player_to_actor(*player);
                step::apply_player_input_cmd(&mut actor, &idle_cmd, &collision_view, physics, dt);
                *player = actor_to_player(player.player_id, actor, player.mounted);
            }
        }
    }

    pub fn ensure_player(&mut self, player_id: PlayerNetId, spawn_position: glam::Vec3) {
        if self.world.player(player_id).is_some() {
            return;
        }
        self.world.players.push(PlayerState {
            player_id,
            position: spawn_position,
            velocity: glam::Vec3::ZERO,
            yaw: -std::f32::consts::FRAC_PI_2,
            pitch: -0.35,
            on_ground: false,
            mounted: None,
        });
    }

    pub fn ensure_vehicle(&mut self, vehicle_id: u32, position: glam::Vec3) {
        if self.world.vehicle(vehicle_id).is_some() {
            return;
        }
        self.world.vehicles.push(VehicleState {
            vehicle_id,
            position,
            ..VehicleState::default()
        });
    }
}

fn player_to_actor(player: PlayerState) -> ActorState {
    ActorState {
        motion: crate::game::actor::MotionState {
            position: player.position,
            velocity: player.velocity,
        },
        look: crate::game::actor::LookState {
            yaw: player.yaw,
            pitch: player.pitch,
        },
        on_ground: player.on_ground,
    }
}

fn actor_to_player(
    player_id: PlayerNetId,
    actor: ActorState,
    mounted: Option<crate::shared::sim::state::MountedState>,
) -> PlayerState {
    PlayerState {
        player_id,
        position: actor.motion.position,
        velocity: actor.motion.velocity,
        yaw: actor.look.yaw,
        pitch: actor.look.pitch,
        on_ground: actor.on_ground,
        mounted,
    }
}
