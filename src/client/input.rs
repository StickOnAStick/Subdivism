use crate::shared::net::protocol::{
    ControlTarget, InputAxes, InputButtons, InputSeq, PlayerInputCmd, PlayerNetId, Tick,
    VehicleNetId,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct RawInputFrame {
    pub forward: f32,
    pub strafe: f32,
    pub throttle: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    pub jump: bool,
    pub sprint: bool,
    pub brake: bool,
    pub interact: bool,
    pub primary_action: bool,
    pub look_delta_yaw_radians: f32,
    pub look_delta_pitch_radians: f32,
}

#[derive(Debug)]
pub struct InputCommandSequencer {
    local_player_id: PlayerNetId,
    next_seq: InputSeq,
}

impl InputCommandSequencer {
    pub fn new(local_player_id: PlayerNetId) -> Self {
        Self {
            local_player_id,
            next_seq: 1,
        }
    }

    pub fn local_player_id(&self) -> PlayerNetId {
        self.local_player_id
    }

    pub fn build_on_foot_command(&mut self, tick: Tick, input: RawInputFrame) -> PlayerInputCmd {
        self.build_command(tick, ControlTarget::OnFoot, input)
    }

    pub fn build_vehicle_pilot_command(
        &mut self,
        tick: Tick,
        vehicle_id: VehicleNetId,
        seat_index: u8,
        input: RawInputFrame,
    ) -> PlayerInputCmd {
        self.build_command(
            tick,
            ControlTarget::VehiclePilot {
                vehicle_id,
                seat_index,
            },
            input,
        )
    }

    fn build_command(
        &mut self,
        tick: Tick,
        target: ControlTarget,
        input: RawInputFrame,
    ) -> PlayerInputCmd {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1).max(1);

        PlayerInputCmd {
            seq,
            tick,
            player_id: self.local_player_id,
            target,
            axes: InputAxes {
                forward: quantize_axis(input.forward),
                strafe: quantize_axis(input.strafe),
                throttle: quantize_axis(input.throttle),
                yaw: quantize_axis(input.yaw),
                pitch: quantize_axis(input.pitch),
                roll: quantize_axis(input.roll),
            },
            buttons: InputButtons {
                jump: input.jump,
                sprint: input.sprint,
                brake: input.brake,
                interact: input.interact,
                primary_action: input.primary_action,
            },
            look_delta_yaw: quantize_look_delta(input.look_delta_yaw_radians),
            look_delta_pitch: quantize_look_delta(input.look_delta_pitch_radians),
        }
    }
}

fn quantize_axis(value: f32) -> i8 {
    (value.clamp(-1.0, 1.0) * i8::MAX as f32).round() as i8
}

fn quantize_look_delta(value_radians: f32) -> i16 {
    // Matches shared::sim::step::LOOK_DELTA_SCALE_RADIANS.
    let scaled = value_radians * 4096.0;
    scaled.clamp(i16::MIN as f32, i16::MAX as f32).round() as i16
}
