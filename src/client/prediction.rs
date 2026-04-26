use std::{collections::VecDeque, time::Duration};

use crate::shared::net::protocol::{InputPacket, InputSeq, PlayerInputCmd};

#[derive(Debug)]
pub struct ClientPredictionState {
    pending_inputs: VecDeque<PlayerInputCmd>,
    outbound_packets: VecDeque<InputPacket>,
    send_interval: Duration,
    send_accumulator: Duration,
    max_commands_per_packet: usize,
    last_acked_seq: InputSeq,
    last_sent_seq: InputSeq,
}

impl ClientPredictionState {
    pub fn new(send_rate_hz: u32, max_commands_per_packet: usize) -> Self {
        let clamped_send_hz = send_rate_hz.max(1);
        Self {
            pending_inputs: VecDeque::new(),
            outbound_packets: VecDeque::new(),
            send_interval: Duration::from_secs_f32(1.0 / clamped_send_hz as f32),
            send_accumulator: Duration::ZERO,
            max_commands_per_packet: max_commands_per_packet.max(1),
            last_acked_seq: 0,
            last_sent_seq: 0,
        }
    }

    pub fn record_local_input(&mut self, cmd: PlayerInputCmd) {
        self.pending_inputs.push_back(cmd);
    }

    pub fn acknowledge_through(&mut self, ack_seq: InputSeq) {
        if ack_seq <= self.last_acked_seq {
            return;
        }
        self.last_acked_seq = ack_seq;

        while let Some(front) = self.pending_inputs.front() {
            if front.seq <= ack_seq {
                self.pending_inputs.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn tick_network_send(&mut self, frame_dt: Duration) {
        self.send_accumulator = (self.send_accumulator + frame_dt).min(self.send_interval * 4);
        while self.send_accumulator >= self.send_interval {
            self.send_accumulator -= self.send_interval;
            if let Some(packet) = self.build_packet_from_pending() {
                self.last_sent_seq = packet.newest_seq;
                self.outbound_packets.push_back(packet);
            }
        }
    }

    pub fn pop_outbound_packet(&mut self) -> Option<InputPacket> {
        self.outbound_packets.pop_front()
    }

    pub fn pending_inputs(&self) -> &VecDeque<PlayerInputCmd> {
        &self.pending_inputs
    }

    pub fn pending_input_count(&self) -> usize {
        self.pending_inputs.len()
    }

    pub fn staged_packet_count(&self) -> usize {
        self.outbound_packets.len()
    }

    pub fn last_sent_seq(&self) -> InputSeq {
        self.last_sent_seq
    }

    pub fn last_acked_seq(&self) -> InputSeq {
        self.last_acked_seq
    }

    fn build_packet_from_pending(&self) -> Option<InputPacket> {
        let newest = self.pending_inputs.back()?.seq;
        let mut commands: Vec<PlayerInputCmd> = self
            .pending_inputs
            .iter()
            .rev()
            .take(self.max_commands_per_packet)
            .copied()
            .collect();
        commands.reverse();

        Some(InputPacket {
            newest_seq: newest,
            commands,
        })
    }
}
