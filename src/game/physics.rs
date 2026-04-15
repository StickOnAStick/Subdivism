use glam::Vec3;

use super::{actor::ActorState, world::World};

pub const PLAYER_RADIUS: f32 = 0.32;
pub const PLAYER_HEIGHT: f32 = 1.8;

pub struct PhysicsConfig {
    pub player_move_speed: f32,
    pub player_sprint_multiplier: f32,
    pub player_jump_speed: f32,
    pub gravity: f32,
    pub collision_step: f32,
    pub respawn_margin: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            player_move_speed: 6.0,
            player_sprint_multiplier: 1.8,
            player_jump_speed: 8.5,
            gravity: 26.0,
            collision_step: 0.05,
            respawn_margin: 32.0,
        }
    }
}

#[derive(Default)]
pub struct MovementInput {
    pub forward: f32,
    pub strafe: f32,
    pub jump_pressed: bool,
    pub sprint_held: bool,
}

pub fn update_player(
    actor: &mut ActorState,
    input: &MovementInput,
    world: &World,
    physics: &PhysicsConfig,
    dt: f32,
) {
    let forward_flat = actor.forward_flat();
    let right_flat = forward_flat.cross(Vec3::Y).normalize_or_zero();
    let wish_dir = forward_flat * input.forward + right_flat * input.strafe;
    let sprint_factor = if input.sprint_held {
        physics.player_sprint_multiplier
    } else {
        1.0
    };
    let horizontal_velocity = if wish_dir.length_squared() > 0.0 {
        wish_dir.normalize() * physics.player_move_speed * sprint_factor
    } else {
        Vec3::ZERO
    };

    if input.jump_pressed && actor.on_ground {
        actor.motion.velocity.y = physics.player_jump_speed;
        actor.on_ground = false;
    }

    actor.motion.velocity.x = horizontal_velocity.x;
    actor.motion.velocity.z = horizontal_velocity.z;
    actor.motion.velocity.y -= physics.gravity * dt;
    actor.on_ground = false;

    actor.motion.position = move_axis(
        world,
        actor.motion.position,
        Vec3::new(actor.motion.velocity.x * dt, 0.0, 0.0),
        physics.collision_step,
        None,
        &mut actor.on_ground,
    );
    actor.motion.position = move_axis(
        world,
        actor.motion.position,
        Vec3::new(0.0, 0.0, actor.motion.velocity.z * dt),
        physics.collision_step,
        None,
        &mut actor.on_ground,
    );
    actor.motion.position = move_axis(
        world,
        actor.motion.position,
        Vec3::new(0.0, actor.motion.velocity.y * dt, 0.0),
        physics.collision_step,
        Some(&mut actor.motion.velocity.y),
        &mut actor.on_ground,
    );

    if world.is_out_of_bounds(actor.motion.position, physics.respawn_margin) {
        actor.motion.position = world.spawn_point();
        actor.motion.velocity = Vec3::ZERO;
        actor.on_ground = false;
    }
}

fn move_axis(
    world: &World,
    mut position: Vec3,
    delta: Vec3,
    collision_step: f32,
    mut velocity_component: Option<&mut f32>,
    on_ground: &mut bool,
) -> Vec3 {
    let distance = delta.length();
    if distance <= f32::EPSILON {
        return position;
    }

    let steps = (distance / collision_step).ceil() as i32;
    let step_delta = delta / steps as f32;

    for _ in 0..steps {
        let candidate = position + step_delta;
        if actor_collides(world, candidate) {
            if step_delta.y < 0.0 {
                *on_ground = true;
            }
            if let Some(component) = velocity_component.as_deref_mut() {
                *component = 0.0;
            }
            break;
        }
        position = candidate;
    }

    position
}

fn actor_collides(world: &World, position: Vec3) -> bool {
    let min = Vec3::new(
        position.x - PLAYER_RADIUS,
        position.y,
        position.z - PLAYER_RADIUS,
    );
    let max = Vec3::new(
        position.x + PLAYER_RADIUS,
        position.y + PLAYER_HEIGHT,
        position.z + PLAYER_RADIUS,
    );

    let min_x = min.x.floor() as i32;
    let max_x = max.x.ceil() as i32 - 1;
    let min_y = min.y.floor() as i32;
    let max_y = max.y.ceil() as i32 - 1;
    let min_z = min.z.floor() as i32;
    let max_z = max.z.ceil() as i32 - 1;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            for z in min_z..=max_z {
                if world.is_solid(x, y, z) {
                    return true;
                }
            }
        }
    }

    false
}
