//! Discord IPC framing (Taiga `share/discord_ipc.cpp`). See
//! <https://docs.discord.com/developers/topics/rpc>.
//!
//! A frame is an opcode and a length, both 32-bit little-endian, then that
//! many bytes of JSON.

use serde_json::Value;

/// Discord listens on the first free of `discord-ipc-0` … `discord-ipc-9`.
pub const SOCKET_COUNT: usize = 10;
pub const HEADER_SIZE: usize = 8;
pub const MAX_FRAME_SIZE: usize = 1024 * 1024;
/// Close code for an unknown application ID.
pub const INVALID_CLIENT_ID: i64 = 4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    Handshake = 0,
    Frame = 1,
    Close = 2,
    Ping = 3,
    Pong = 4,
}

impl Opcode {
    pub fn from_i32(value: i32) -> Option<Self> {
        Some(match value {
            0 => Self::Handshake,
            1 => Self::Frame,
            2 => Self::Close,
            3 => Self::Ping,
            4 => Self::Pong,
            _ => return None,
        })
    }
}

/// The socket path for `index`: a named pipe on Windows, else a Unix socket
/// in the runtime or temporary folder.
pub fn socket_name(prefix: &str, index: usize) -> String {
    let name = format!("{prefix}{index}");
    if cfg!(windows) {
        return format!(r"\\.\pipe\{name}");
    }
    let dir = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|d| !d.is_empty()))
        .unwrap_or_else(|| "/tmp".into());
    format!("{dir}/{name}")
}

pub fn encode(opcode: Opcode, payload: &Value) -> Vec<u8> {
    let json = serde_json::to_vec(payload).unwrap_or_default();
    let mut frame = Vec::with_capacity(HEADER_SIZE + json.len());
    frame.extend_from_slice(&(opcode as i32).to_le_bytes());
    frame.extend_from_slice(&(json.len() as i32).to_le_bytes());
    frame.extend_from_slice(&json);
    frame
}

#[derive(Debug, PartialEq, Eq)]
pub struct InvalidFrame(pub i64);

/// Collects bytes and splits them into frames.
#[derive(Debug, Default)]
pub struct FrameReader {
    buffer: Vec<u8>,
}

impl FrameReader {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// The next complete frame, if any. Unknown opcodes are returned as `None`
    /// in the first position so they can be skipped.
    #[allow(clippy::type_complexity)]
    pub fn next_frame(&mut self) -> Result<Option<(Option<Opcode>, Value)>, InvalidFrame> {
        if self.buffer.len() < HEADER_SIZE {
            return Ok(None);
        }
        let opcode = i32::from_le_bytes(self.buffer[0..4].try_into().unwrap());
        let length = i32::from_le_bytes(self.buffer[4..8].try_into().unwrap());
        if length < 0 || length as usize > MAX_FRAME_SIZE {
            return Err(InvalidFrame(i64::from(length)));
        }
        let end = HEADER_SIZE + length as usize;
        if self.buffer.len() < end {
            return Ok(None);
        }
        let payload: Value =
            serde_json::from_slice(&self.buffer[HEADER_SIZE..end]).unwrap_or(Value::Null);
        self.buffer.drain(..end);
        Ok(Some((Opcode::from_i32(opcode), payload)))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn round_trip_in_pieces() {
        let mut bytes = encode(Opcode::Frame, &json!({ "cmd": "DISPATCH", "evt": "READY" }));
        bytes.extend(encode(Opcode::Ping, &json!({})));
        let mut reader = FrameReader::default();
        reader.push(&bytes[..5]);
        assert_eq!(reader.next_frame(), Ok(None));
        reader.push(&bytes[5..]);
        let (opcode, payload) = reader.next_frame().unwrap().unwrap();
        assert_eq!(opcode, Some(Opcode::Frame));
        assert_eq!(payload["evt"], "READY");
        assert_eq!(reader.next_frame().unwrap().unwrap().0, Some(Opcode::Ping));
        assert_eq!(reader.next_frame(), Ok(None));
    }

    #[test]
    fn rejects_bad_lengths() {
        let mut reader = FrameReader::default();
        let mut bytes = 1i32.to_le_bytes().to_vec();
        bytes.extend((-5i32).to_le_bytes());
        reader.push(&bytes);
        assert_eq!(reader.next_frame(), Err(InvalidFrame(-5)));
    }

    #[test]
    fn socket_names() {
        let name = socket_name("discord-ipc-", 3);
        assert!(name.ends_with("discord-ipc-3"));
        if cfg!(windows) {
            assert_eq!(name, r"\\.\pipe\discord-ipc-3");
        }
    }
}
