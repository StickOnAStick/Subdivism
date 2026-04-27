use bincode::{
    config,
    error::{DecodeError, EncodeError},
    serde::{decode_from_slice, encode_to_vec},
};
use serde::{Deserialize, Serialize};

use crate::shared::net::protocol::{InputPacket, PlayerNetId, SnapshotPacket};

const PROTOCOL_VERSION: u16 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClientWireMessage {
    Hello { protocol_version: u16 },
    Input(InputPacket),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerWireMessage {
    Snapshot {
        assigned_player_id: PlayerNetId,
        packet: SnapshotPacket,
    },
}

pub fn hello_message() -> ClientWireMessage {
    ClientWireMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
    }
}

pub fn encode_client_message(message: &ClientWireMessage) -> Result<Vec<u8>, EncodeError> {
    encode_to_vec(message, config::standard())
}

pub fn decode_client_message(bytes: &[u8]) -> Result<ClientWireMessage, DecodeError> {
    decode_from_slice(bytes, config::standard()).map(|(message, _)| message)
}

pub fn encode_server_message(message: &ServerWireMessage) -> Result<Vec<u8>, EncodeError> {
    encode_to_vec(message, config::standard())
}

pub fn decode_server_message(bytes: &[u8]) -> Result<ServerWireMessage, DecodeError> {
    decode_from_slice(bytes, config::standard()).map(|(message, _)| message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::net::protocol::SnapshotPacket;

    #[test]
    fn wire_roundtrip_server_snapshot() {
        let original = ServerWireMessage::Snapshot {
            assigned_player_id: 7,
            packet: SnapshotPacket::default(),
        };
        let bytes = encode_server_message(&original).expect("encode snapshot");
        let decoded = decode_server_message(&bytes).expect("decode snapshot");
        match decoded {
            ServerWireMessage::Snapshot {
                assigned_player_id,
                packet,
            } => {
                assert_eq!(assigned_player_id, 7);
                assert_eq!(packet.server_tick, 0);
            }
        }
    }
}
