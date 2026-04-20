use glam::{Vec2, Vec3};

use super::{actor::ActorState, world::World};

pub const PLAYER_RADIUS: f32 = 0.32;
pub const PLAYER_HEIGHT: f32 = 1.8;

pub struct PhysicsConfig {
    pub player_move_speed: f32,
    pub player_sprint_multiplier: f32,
    pub player_move_acceleration: f32,
    pub player_move_deceleration: f32,
    pub player_air_acceleration: f32,
    pub player_air_deceleration: f32,
    pub player_landing_carry_seconds: f32,
    pub player_jump_speed: f32,
    pub gravity: f32,
    pub water_move_speed_scale: f32,
    pub water_sprint_multiplier: f32,
    pub water_gravity_scale: f32,
    pub water_drag: f32,
    pub water_buoyancy: f32,
    pub swim_up_speed: f32,
    pub collision_step: f32,
    pub respawn_margin: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            player_move_speed: 6.0,
            player_sprint_multiplier: 1.8,
            player_move_acceleration: 30.0,
            player_move_deceleration: 10.0,
            player_air_acceleration: 10.0,
            player_air_deceleration: 2.5,
            player_landing_carry_seconds: 0.10,
            player_jump_speed: 8.5,
            gravity: 26.0,
            water_move_speed_scale: 0.58,
            water_sprint_multiplier: 1.2,
            water_gravity_scale: 0.38,
            water_drag: 4.8,
            water_buoyancy: 11.0,
            swim_up_speed: 4.6,
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
    let was_on_ground = actor.on_ground;
    if was_on_ground {
        actor.landing_velocity_carry_timer = (actor.landing_velocity_carry_timer - dt).max(0.0);
    }
    let water_submersion = sample_water_submersion(world, actor.motion.position);
    let in_water = water_submersion > 0.05;
    let forward_flat = actor.forward_flat();
    let right_flat = forward_flat.cross(Vec3::Y).normalize_or_zero();
    let wish_dir = forward_flat * input.forward + right_flat * input.strafe;
    let sprint_factor = if input.sprint_held {
        if in_water {
            physics.water_sprint_multiplier
        } else {
            physics.player_sprint_multiplier
        }
    } else {
        1.0
    };
    let move_speed = if in_water {
        physics.player_move_speed * physics.water_move_speed_scale
    } else {
        physics.player_move_speed
    };
    let target_horizontal_velocity = if wish_dir.length_squared() > 0.0 {
        let wish_norm = wish_dir.normalize();
        Vec2::new(wish_norm.x, wish_norm.z) * move_speed * sprint_factor
    } else {
        Vec2::ZERO
    };
    let mut horizontal_velocity = Vec2::new(actor.motion.velocity.x, actor.motion.velocity.z);
    let use_ground_carry = was_on_ground
        && actor.landing_velocity_carry_timer > 0.0
        && !in_water
        && physics.player_landing_carry_seconds > 0.0;
    if was_on_ground && !use_ground_carry && !in_water {
        // Grounded controls are immediate: no horizontal inertia during normal walk/run.
        horizontal_velocity = target_horizontal_velocity;
    } else {
        let surface_friction_scale = if in_water || !was_on_ground {
            1.0
        } else {
            sample_surface_friction(world, actor.motion.position)
        };
        let (acceleration, deceleration) = if in_water {
            (
                physics.player_move_acceleration * physics.water_move_speed_scale,
                physics.player_move_deceleration * physics.water_move_speed_scale,
            )
        } else if was_on_ground {
            (
                physics.player_move_acceleration * surface_friction_scale,
                physics.player_move_deceleration * surface_friction_scale,
            )
        } else {
            (
                physics.player_air_acceleration,
                physics.player_air_deceleration,
            )
        };
        let max_delta = if target_horizontal_velocity.length_squared() > 0.0 {
            acceleration.max(0.0) * dt
        } else {
            deceleration.max(0.0) * dt
        };
        horizontal_velocity =
            move_towards_vec2(horizontal_velocity, target_horizontal_velocity, max_delta);
    }

    if in_water {
        if input.jump_pressed {
            actor.motion.velocity.y = actor.motion.velocity.y.max(physics.swim_up_speed);
        }
    } else if input.jump_pressed && actor.on_ground {
        actor.motion.velocity.y = physics.player_jump_speed;
        actor.on_ground = false;
    }

    actor.motion.velocity.x = horizontal_velocity.x;
    actor.motion.velocity.z = horizontal_velocity.y;
    let gravity_scale = if in_water {
        physics.water_gravity_scale
    } else {
        1.0
    };
    actor.motion.velocity.y -= physics.gravity * gravity_scale * dt;
    if in_water {
        actor.motion.velocity.y += physics.water_buoyancy * water_submersion * dt;
        let drag = (1.0 - physics.water_drag * dt).clamp(0.0, 1.0);
        actor.motion.velocity.x *= drag;
        actor.motion.velocity.y *= drag;
        actor.motion.velocity.z *= drag;
    }
    actor.on_ground = false;

    actor.motion.position = move_axis(
        world,
        actor.motion.position,
        Vec3::new(actor.motion.velocity.x * dt, 0.0, 0.0),
        physics.collision_step,
        Some(&mut actor.motion.velocity.x),
        &mut actor.on_ground,
    );
    actor.motion.position = move_axis(
        world,
        actor.motion.position,
        Vec3::new(0.0, 0.0, actor.motion.velocity.z * dt),
        physics.collision_step,
        Some(&mut actor.motion.velocity.z),
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
    if !was_on_ground && actor.on_ground {
        actor.landing_velocity_carry_timer = physics.player_landing_carry_seconds.max(0.0);
    } else if !actor.on_ground {
        actor.landing_velocity_carry_timer = 0.0;
    }

    if world.is_out_of_bounds(actor.motion.position, physics.respawn_margin) {
        actor.motion.position = world.spawn_point();
        actor.motion.velocity = Vec3::ZERO;
        actor.on_ground = false;
        actor.landing_velocity_carry_timer = 0.0;
    }
}

fn move_towards_vec2(current: Vec2, target: Vec2, max_delta: f32) -> Vec2 {
    if max_delta <= 0.0 {
        return current;
    }
    let delta = target - current;
    let distance = delta.length();
    if distance <= max_delta || distance <= f32::EPSILON {
        target
    } else {
        current + delta / distance * max_delta
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
                for (sub, block) in world.sub_blocks_in_cell(x as i64, y, z as i64) {
                    if !block.is_solid() {
                        continue;
                    }
                    let step = 1.0 / sub.divisions as f32;
                    let sub_min = Vec3::new(
                        sub.x as f32 + sub.sx as f32 * step,
                        sub.y as f32 + sub.sy as f32 * step,
                        sub.z as f32 + sub.sz as f32 * step,
                    );
                    let sub_max = sub_min + Vec3::splat(step);
                    if sub_min.x < max.x
                        && sub_max.x > min.x
                        && sub_min.y < max.y
                        && sub_max.y > min.y
                        && sub_min.z < max.z
                        && sub_max.z > min.z
                    {
                        return true;
                    }
                }
            }
        }
    }

    false
}

fn sample_water_submersion(world: &World, position: Vec3) -> f32 {
    let sample_offsets = [
        Vec3::new(0.0, 0.12, 0.0),
        Vec3::new(PLAYER_RADIUS * 0.72, PLAYER_HEIGHT * 0.35, 0.0),
        Vec3::new(-PLAYER_RADIUS * 0.72, PLAYER_HEIGHT * 0.35, 0.0),
        Vec3::new(0.0, PLAYER_HEIGHT * 0.62, PLAYER_RADIUS * 0.72),
        Vec3::new(0.0, PLAYER_HEIGHT * 0.62, -PLAYER_RADIUS * 0.72),
        Vec3::new(0.0, PLAYER_HEIGHT * 0.88, 0.0),
    ];

    let mut water_hits = 0_u32;
    for offset in sample_offsets.iter().copied() {
        let probe = position + offset;
        if world.is_water_i64(
            probe.x.floor() as i64,
            probe.y.floor() as i32,
            probe.z.floor() as i64,
        ) {
            water_hits += 1;
        }
    }

    water_hits as f32 / sample_offsets.len() as f32
}

fn sample_surface_friction(world: &World, position: Vec3) -> f32 {
    let y = (position.y - 0.05).floor() as i32;
    let probes = [
        (position.x, position.z),
        (position.x + PLAYER_RADIUS * 0.66, position.z),
        (position.x - PLAYER_RADIUS * 0.66, position.z),
        (position.x, position.z + PLAYER_RADIUS * 0.66),
        (position.x, position.z - PLAYER_RADIUS * 0.66),
    ];
    let mut sum = 0.0_f32;
    let mut count = 0_u32;
    for (x, z) in probes {
        let block = world.block_at_i64(x.floor() as i64, y, z.floor() as i64);
        if block.is_solid() {
            sum += block.properties().friction.clamp(0.15, 2.0);
            count += 1;
        }
    }
    if count == 0 {
        1.0
    } else {
        (sum / count as f32).clamp(0.15, 2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{
        actor::{ActorId, ActorState},
        world::Block,
    };

    fn setup_flat_test_world() -> World {
        let mut world = World::generate_default();
        for y in -2..=8 {
            for x in -12..=12 {
                for z in -12..=12 {
                    let block = if y == 0 { Block::Stone } else { Block::Air };
                    world.set_block_i64(x, y, z, block);
                }
            }
        }
        world
    }

    fn horizontal_speed(velocity: Vec3) -> f32 {
        Vec2::new(velocity.x, velocity.z).length()
    }

    #[test]
    fn grounded_velocity_stops_immediately_without_input() {
        let world = setup_flat_test_world();
        let mut actor = ActorState::player(ActorId(0), Vec3::new(0.5, 1.0, 0.5));
        actor.on_ground = true;
        let physics = PhysicsConfig::default();
        let dt = 1.0 / 60.0;

        let moving_input = MovementInput {
            forward: 1.0,
            strafe: 0.0,
            jump_pressed: false,
            sprint_held: false,
        };
        update_player(&mut actor, &moving_input, &world, &physics, dt);
        let speed_after_press = horizontal_speed(actor.motion.velocity);
        assert!(
            speed_after_press > 0.01,
            "expected player to gain horizontal speed"
        );

        let idle_input = MovementInput::default();
        update_player(&mut actor, &idle_input, &world, &physics, dt);
        let speed_after_release = horizontal_speed(actor.motion.velocity);
        assert!(
            speed_after_release < 0.001,
            "expected immediate ground stop after release, but speed was {}",
            speed_after_release
        );
    }

    #[test]
    fn airborne_velocity_carries_without_input() {
        let world = setup_flat_test_world();
        let mut actor = ActorState::player(ActorId(0), Vec3::new(0.5, 3.0, 0.5));
        actor.on_ground = false;
        actor.motion.velocity.x = 4.0;
        let physics = PhysicsConfig::default();
        let dt = 1.0 / 60.0;
        let idle_input = MovementInput::default();

        update_player(&mut actor, &idle_input, &world, &physics, dt);
        let speed_after_air_step = horizontal_speed(actor.motion.velocity);
        assert!(
            speed_after_air_step > 3.8,
            "expected air momentum carry, got {}",
            speed_after_air_step
        );
        assert!(
            !actor.on_ground,
            "actor should still be airborne after one short air step"
        );
    }

    #[test]
    fn landing_grace_keeps_carry_then_ground_control_snaps() {
        let world = setup_flat_test_world();
        let mut actor = ActorState::player(ActorId(0), Vec3::new(0.5, 2.2, 0.5));
        actor.on_ground = false;
        actor.motion.velocity = Vec3::new(4.5, -8.0, 0.0);
        let physics = PhysicsConfig::default();
        let dt = 1.0 / 60.0;
        let idle_input = MovementInput::default();

        let mut landed = false;
        for _ in 0..120 {
            update_player(&mut actor, &idle_input, &world, &physics, dt);
            if actor.on_ground {
                landed = true;
                break;
            }
        }
        assert!(landed, "expected actor to land during test window");
        assert!(
            actor.landing_velocity_carry_timer > 0.0,
            "expected landing carry timer to be set on landing"
        );

        update_player(&mut actor, &idle_input, &world, &physics, dt);
        let speed_during_carry = horizontal_speed(actor.motion.velocity);
        assert!(
            speed_during_carry > 0.05,
            "expected immediate post-landing carry, got {}",
            speed_during_carry
        );

        let settle_frames = ((physics.player_landing_carry_seconds / dt).ceil() as usize) + 4;
        for _ in 0..settle_frames {
            update_player(&mut actor, &idle_input, &world, &physics, dt);
        }
        let final_speed = horizontal_speed(actor.motion.velocity);
        assert!(
            final_speed < 0.05,
            "expected snap-to-ground control after carry window, got speed {}",
            final_speed
        );
    }
}
