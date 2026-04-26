use glam::Vec3;
use winit::keyboard::KeyCode;

use crate::game::physics::{self, MovementInput};

use super::{App, CameraMode, FREE_CAMERA_SPEED, PLAYER_EYE_HEIGHT, UiMode};
use super::components::{InputCommandFrame, QueuedServerCommand, SessionNetMode};

impl App {
    pub(super) fn update(&mut self, dt: f32) {
        if self.menu.ui_mode != UiMode::Playing {
            return;
        }

        match self.camera.mode {
            CameraMode::Player | CameraMode::Chase => self.update_player(dt),
            CameraMode::Free => self.update_free_camera(dt),
        }

        if self.camera.mode == CameraMode::Chase {
            self.update_chase_camera(dt);
        }

        self.sync_active_camera();
    }

    fn movement_input_from_pressed_keys(&self) -> MovementInput {
        MovementInput {
            forward: super::axis_value(self.input.key(KeyCode::KeyW), self.input.key(KeyCode::KeyS)),
            strafe: super::axis_value(self.input.key(KeyCode::KeyD), self.input.key(KeyCode::KeyA)),
            jump_pressed: self.input.key(KeyCode::Space),
            sprint_held: self.input.key(KeyCode::ShiftLeft) || self.input.key(KeyCode::ShiftRight),
        }
    }

    fn next_input_command(&mut self, dt: f32) -> InputCommandFrame {
        let seq = self.netcode.prediction.next_sequence;
        self.netcode.prediction.next_sequence = self.netcode.prediction.next_sequence.wrapping_add(1);
        let actor = self.actors.local_player();
        InputCommandFrame {
            sequence: seq,
            dt,
            movement: self.movement_input_from_pressed_keys(),
            yaw: actor.look.yaw,
            pitch: actor.look.pitch,
        }
    }

    fn update_player(&mut self, dt: f32) {
        if !self.netcode.active_mode.prediction_enabled() {
            let input = self.movement_input_from_pressed_keys();
            physics::update_player(
                self.actors.local_player_mut(),
                &input,
                &self.world,
                &self.physics,
                dt,
            );
            self.netcode.reset_for_actor(*self.actors.local_player());
            return;
        }

        let command = self.next_input_command(dt);
        physics::update_player(
            self.actors.local_player_mut(),
            &command.movement,
            &self.world,
            &self.physics,
            command.dt,
        );
        self.netcode.prediction.pending_local_inputs.push_back(command);
        self.netcode.queued_server_inputs.push_back(QueuedServerCommand {
            frames_left: self.netcode.simulated_latency_frames,
            command,
        });
        self.step_authoritative_server_simulation();
        self.reconcile_to_authoritative_state();
    }

    fn step_authoritative_server_simulation(&mut self) {
        if self.netcode.active_mode == SessionNetMode::Solo {
            return;
        }

        let mut remaining = self.netcode.queued_server_inputs.len();
        while remaining > 0 {
            remaining -= 1;
            let Some(mut queued) = self.netcode.queued_server_inputs.pop_front() else {
                break;
            };

            if queued.frames_left > 0 {
                queued.frames_left -= 1;
                self.netcode.queued_server_inputs.push_back(queued);
                continue;
            }

            self.netcode.server_actor.look.yaw = queued.command.yaw;
            self.netcode.server_actor.look.pitch = queued.command.pitch;
            physics::update_player(
                &mut self.netcode.server_actor,
                &queued.command.movement,
                &self.world,
                &self.physics,
                queued.command.dt,
            );
            self.netcode.prediction.last_authoritative_sequence = queued.command.sequence;
        }
    }

    fn reconcile_to_authoritative_state(&mut self) {
        let authoritative_seq = self.netcode.prediction.last_authoritative_sequence;
        while self
            .netcode
            .prediction
            .pending_local_inputs
            .front()
            .is_some_and(|pending| pending.sequence <= authoritative_seq)
        {
            self.netcode.prediction.pending_local_inputs.pop_front();
        }

        let authoritative = self.netcode.server_actor;
        let actor = self.actors.local_player_mut();
        let delta = authoritative.motion.position - actor.motion.position;
        let error = delta.length();
        self.netcode.prediction.last_position_error = error;
        if error <= f32::EPSILON {
            return;
        }

        if error > self.netcode.prediction.snap_distance {
            actor.motion.position = authoritative.motion.position;
            actor.motion.velocity = authoritative.motion.velocity;
            actor.on_ground = authoritative.on_ground;
            return;
        }

        let gain = self.netcode.prediction.correction_gain.clamp(0.0, 1.0);
        actor.motion.position += delta * gain;
        actor.motion.velocity = actor
            .motion
            .velocity
            .lerp(authoritative.motion.velocity, gain * 0.6);
    }

    fn update_free_camera(&mut self, dt: f32) {
        let forward = self.camera.free_camera.forward();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let mut move_dir = Vec3::ZERO;

        if self.input.key(KeyCode::KeyW) {
            move_dir += forward;
        }
        if self.input.key(KeyCode::KeyS) {
            move_dir -= forward;
        }
        if self.input.key(KeyCode::KeyA) {
            move_dir -= right;
        }
        if self.input.key(KeyCode::KeyD) {
            move_dir += right;
        }
        if self.input.key(KeyCode::Space) {
            move_dir += Vec3::Y;
        }
        if self.input.key(KeyCode::ShiftLeft) || self.input.key(KeyCode::ShiftRight) {
            move_dir -= Vec3::Y;
        }

        if move_dir.length_squared() > 0.0 {
            self.camera.free_camera.position += move_dir.normalize() * FREE_CAMERA_SPEED * dt;
        }

        if self.world.is_out_of_bounds(
            self.camera.free_camera.position - Vec3::Y * PLAYER_EYE_HEIGHT,
            self.physics.respawn_margin,
        ) {
            self.camera.free_camera.position =
                self.world.spawn_point() + Vec3::Y * PLAYER_EYE_HEIGHT;
        }
    }

    fn update_chase_camera(&mut self, dt: f32) {
        let actor = self.actors.local_player();
        let target = actor.motion.position + Vec3::Y * (PLAYER_EYE_HEIGHT + self.camera.chase_height)
            - actor.forward_flat() * self.camera.chase_distance;
        let gain = 1.0 - (-self.camera.chase_smoothing.max(0.1) * dt.max(0.0)).exp();
        self.camera.chase_camera_position = self.camera.chase_camera_position.lerp(target, gain);
    }
}
