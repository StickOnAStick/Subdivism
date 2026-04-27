use glam::{Quat, Vec3};

use crate::{
    game::{
        actor::ActorState,
        physics::{self, MovementInput, PhysicsConfig},
    },
    shared::{
        net::protocol::{ControlTarget, PlayerInputCmd},
        sim::state::VehicleState,
    },
};

pub const DEFAULT_SIM_TICK_HZ: u32 = 60;
pub const PLAYER_MAX_PITCH: f32 = 1.54;
pub const LOOK_DELTA_SCALE_RADIANS: f32 = 1.0 / 4096.0;

pub fn fixed_dt_seconds(tick_hz: u32) -> f32 {
    let clamped_hz = tick_hz.max(1);
    1.0 / clamped_hz as f32
}

pub fn axis_i8_to_f32(value: i8) -> f32 {
    (value as f32 / i8::MAX as f32).clamp(-1.0, 1.0)
}

pub fn apply_player_input_cmd<C: physics::CollisionWorld>(
    actor: &mut ActorState,
    cmd: &PlayerInputCmd,
    world: &C,
    physics: &PhysicsConfig,
    dt: f32,
) {
    let look_yaw = cmd.look_delta_yaw as f32 * LOOK_DELTA_SCALE_RADIANS;
    let look_pitch = cmd.look_delta_pitch as f32 * LOOK_DELTA_SCALE_RADIANS;
    actor.look.yaw += look_yaw;
    actor.look.pitch = (actor.look.pitch + look_pitch).clamp(-PLAYER_MAX_PITCH, PLAYER_MAX_PITCH);

    match cmd.target {
        ControlTarget::OnFoot => {
            let movement = MovementInput {
                forward: axis_i8_to_f32(cmd.axes.forward),
                strafe: axis_i8_to_f32(cmd.axes.strafe),
                jump_pressed: cmd.buttons.jump,
                sprint_held: cmd.buttons.sprint,
            };
            physics::update_player(actor, &movement, world, physics, dt);
        }
        ControlTarget::VehiclePilot { .. } => {
            // While mounted the actor transform should be seat-driven. For now we stop
            // direct on-foot motion and leave mount pose resolution to the vehicle system.
            actor.motion.velocity = Vec3::ZERO;
        }
    }
}

pub fn apply_vehicle_pilot_input(vehicle: &mut VehicleState, cmd: &PlayerInputCmd, dt: f32) {
    let throttle = axis_i8_to_f32(cmd.axes.throttle);
    let yaw_input = axis_i8_to_f32(cmd.axes.yaw);
    let pitch_input = axis_i8_to_f32(cmd.axes.pitch);
    let roll_input = axis_i8_to_f32(cmd.axes.roll);

    let throttle_accel = 26.0;
    let drag = 0.28;
    let angular_accel = 1.7;
    let angular_damping = 0.86;

    vehicle.throttle = throttle;
    vehicle.linear_velocity +=
        vehicle.orientation * Vec3::new(throttle * throttle_accel * dt, 0.0, 0.0);

    vehicle.angular_velocity += Vec3::new(pitch_input, yaw_input, roll_input) * angular_accel * dt;
    vehicle.angular_velocity *= angular_damping;

    let delta = vehicle.angular_velocity * dt;
    let delta_len = delta.length();
    if delta_len > f32::EPSILON {
        let axis = delta / delta_len;
        vehicle.orientation =
            (Quat::from_axis_angle(axis, delta_len) * vehicle.orientation).normalize();
    }

    if cmd.buttons.brake {
        vehicle.linear_velocity *= 0.84;
    }

    vehicle.linear_velocity -= vehicle.linear_velocity * drag * dt;
    vehicle.position += vehicle.linear_velocity * dt;
}
