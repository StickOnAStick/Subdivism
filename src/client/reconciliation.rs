use crate::{
    game::{actor::ActorState, physics::PhysicsConfig, world::World},
    shared::{
        net::protocol::{ActorSnapshot, PlayerInputCmd},
        sim::step,
    },
};

pub fn reconcile_local_actor(
    actor: &mut ActorState,
    authoritative: &ActorSnapshot,
    pending_inputs: impl IntoIterator<Item = PlayerInputCmd>,
    world: &World,
    physics: &PhysicsConfig,
    dt: f32,
) {
    let collision_view = world.collision_view();
    actor.motion.position = authoritative.position;
    actor.motion.velocity = authoritative.velocity;
    actor.look.yaw = authoritative.yaw;
    actor.look.pitch = authoritative.pitch;
    actor.on_ground = false;

    for cmd in pending_inputs {
        step::apply_player_input_cmd(actor, &cmd, &collision_view, physics, dt);
    }
}
