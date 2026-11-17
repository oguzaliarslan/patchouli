//! Keeps a connection to the Discord app and sets the activity (Taiga
//! `share/discord_ipc.cpp` `Client` and `discord_rpc.cpp` `ActivityManager`).
//!
//! [`Presence`] is a handle; the connection runs in the future returned by
//! [`Presence::new`], which the caller spawns. While enabled it connects,
//! retrying with backoff while Discord isn't running, and sends the activity
//! again after reconnecting. Disabling closes the connection, which makes
//! Discord remove the activity.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::watch;

use super::ipc::{FrameReader, INVALID_CLIENT_ID, Opcode, SOCKET_COUNT, encode, socket_name};

const MIN_RETRY: Duration = Duration::from_secs(2);
const MAX_RETRY: Duration = Duration::from_secs(60);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PresenceStatus {
    #[default]
    Disabled,
    /// Looking for the Discord app.
    Connecting,
    Connected,
    /// Discord doesn't know the application ID; not retried until re-enabled.
    InvalidClientId,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Desired {
    enabled: bool,
    /// The `activity` object; `None` clears it.
    activity: Option<Value>,
}

#[derive(Clone)]
pub struct Presence {
    desired: watch::Sender<Desired>,
    status: Arc<Mutex<PresenceStatus>>,
}

trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

enum End {
    Disabled,
    Disconnected,
    InvalidClientId,
    Dropped,
}

impl Presence {
    /// `socket_prefix` is `discord-ipc-` except in tests.
    pub fn new(
        client_id: String,
        socket_prefix: String,
    ) -> (Self, impl Future<Output = ()> + Send) {
        let (desired, receiver) = watch::channel(Desired::default());
        let status = Arc::new(Mutex::new(PresenceStatus::Disabled));
        let task = run(client_id, socket_prefix, receiver, Arc::clone(&status));
        (Self { desired, status }, task)
    }

    pub fn status(&self) -> PresenceStatus {
        *self.status.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Connects (or disconnects) and sets the activity.
    pub fn set(&self, enabled: bool, activity: Option<Value>) {
        let desired = Desired { enabled, activity };
        // Skip updates that change nothing, to stay under Discord's rate limit.
        self.desired.send_if_modified(|current| {
            if *current == desired {
                false
            } else {
                *current = desired;
                true
            }
        });
    }
}

fn set_status(status: &Mutex<PresenceStatus>, value: PresenceStatus) {
    *status.lock().unwrap_or_else(|e| e.into_inner()) = value;
}

async fn run(
    client_id: String,
    prefix: String,
    mut desired: watch::Receiver<Desired>,
    status: Arc<Mutex<PresenceStatus>>,
) {
    let mut retry = MIN_RETRY;
    loop {
        // Wait until enabled.
        while !desired.borrow_and_update().enabled {
            set_status(&status, PresenceStatus::Disabled);
            if desired.changed().await.is_err() {
                return;
            }
            retry = MIN_RETRY;
        }
        set_status(&status, PresenceStatus::Connecting);
        match session(&client_id, &prefix, &mut desired, &status).await {
            End::Dropped => return,
            End::Disabled => continue,
            End::InvalidClientId => {
                set_status(&status, PresenceStatus::InvalidClientId);
                log::warn!("Discord: unknown application ID");
                // Wait for a settings change before trying again.
                loop {
                    if desired.changed().await.is_err() {
                        return;
                    }
                    if !desired.borrow().enabled {
                        break;
                    }
                }
                continue;
            }
            End::Disconnected => {}
        }
        set_status(&status, PresenceStatus::Connecting);
        // Quickly at first (Discord restarting), then less often.
        tokio::select! {
            () = tokio::time::sleep(retry) => {}
            changed = desired.changed() => {
                if changed.is_err() {
                    return;
                }
            }
        }
        retry = (retry * 2).min(MAX_RETRY);
    }
}

async fn connect(prefix: &str) -> Option<Box<dyn Stream>> {
    for index in 0..SOCKET_COUNT {
        let name = socket_name(prefix, index);
        #[cfg(windows)]
        let stream = tokio::net::windows::named_pipe::ClientOptions::new()
            .open(&name)
            .map(|s| Box::new(s) as Box<dyn Stream>);
        #[cfg(unix)]
        let stream = tokio::net::UnixStream::connect(&name)
            .await
            .map(|s| Box::new(s) as Box<dyn Stream>);
        if let Ok(stream) = stream {
            return Some(stream);
        }
    }
    None
}

fn nonce() -> String {
    let mut bytes = [0u8; 16];
    let _ = getrandom::fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn set_activity_frame(activity: Option<&Value>) -> Vec<u8> {
    let mut args = json!({ "pid": std::process::id() });
    // The activity is cleared if omitted.
    if let Some(activity) = activity {
        args["activity"] = activity.clone();
    }
    encode(
        Opcode::Frame,
        &json!({ "cmd": "SET_ACTIVITY", "args": args, "nonce": nonce() }),
    )
}

/// What a frame from Discord means for the connection.
enum Received {
    Ready,
    Nothing,
    Close(i64),
}

async fn handle_frames(
    reader: &mut FrameReader,
    stream: &mut Box<dyn Stream>,
) -> std::io::Result<Vec<Received>> {
    let mut received = Vec::new();
    loop {
        match reader.next_frame() {
            Ok(None) => return Ok(received),
            Err(invalid) => {
                log::warn!("Discord: invalid frame length {}", invalid.0);
                return Err(std::io::ErrorKind::InvalidData.into());
            }
            Ok(Some((opcode, payload))) => match opcode {
                Some(Opcode::Frame) => {
                    let event = payload["evt"].as_str().unwrap_or_default();
                    if payload["cmd"] == "DISPATCH" && event == "READY" {
                        received.push(Received::Ready);
                    } else if event == "ERROR" {
                        log::warn!(
                            "Discord: error {} {}",
                            payload["data"]["code"],
                            payload["data"]["message"]
                        );
                    }
                }
                Some(Opcode::Close) => {
                    log::info!("Discord: closed {} {}", payload["code"], payload["message"]);
                    received.push(Received::Close(
                        payload["code"].as_i64().unwrap_or_default(),
                    ));
                }
                Some(Opcode::Ping) => {
                    stream.write_all(&encode(Opcode::Pong, &payload)).await?;
                    received.push(Received::Nothing);
                }
                _ => {}
            },
        }
    }
}

async fn session(
    client_id: &str,
    prefix: &str,
    desired: &mut watch::Receiver<Desired>,
    status: &Mutex<PresenceStatus>,
) -> End {
    let Some(mut stream) = connect(prefix).await else {
        return End::Disconnected;
    };
    let handshake = encode(
        Opcode::Handshake,
        &json!({ "v": 1, "client_id": client_id }),
    );
    if stream.write_all(&handshake).await.is_err() {
        return End::Disconnected;
    }

    let mut reader = FrameReader::default();
    let mut buffer = vec![0u8; 8192];

    // Wait for READY.
    let ready = tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
        loop {
            let n = stream.read(&mut buffer).await.ok()?;
            if n == 0 {
                return None;
            }
            reader.push(&buffer[..n]);
            for r in handle_frames(&mut reader, &mut stream).await.ok()? {
                match r {
                    Received::Ready => return Some(Ok(())),
                    Received::Close(code) => return Some(Err(code)),
                    Received::Nothing => {}
                }
            }
        }
    })
    .await;
    match ready {
        Ok(Some(Ok(()))) => {}
        Ok(Some(Err(code))) if code == INVALID_CLIENT_ID => return End::InvalidClientId,
        _ => return End::Disconnected,
    }
    set_status(status, PresenceStatus::Connected);
    log::info!("Discord: connected");

    // A new connection starts without an activity.
    let mut sent: Option<Value> = None;
    let initial = desired.borrow_and_update().activity.clone();
    if initial.is_some() {
        if stream
            .write_all(&set_activity_frame(initial.as_ref()))
            .await
            .is_err()
        {
            return End::Disconnected;
        }
        sent = initial;
    }

    loop {
        tokio::select! {
            changed = desired.changed() => {
                if changed.is_err() {
                    return End::Dropped;
                }
                let current = desired.borrow_and_update().clone();
                if !current.enabled {
                    // Dropping the stream closes it; Discord clears the activity.
                    return End::Disabled;
                }
                if current.activity != sent {
                    if stream.write_all(&set_activity_frame(current.activity.as_ref())).await.is_err() {
                        return End::Disconnected;
                    }
                    sent = current.activity;
                }
            }
            read = stream.read(&mut buffer) => {
                let Ok(n) = read else { return End::Disconnected };
                if n == 0 {
                    return End::Disconnected;
                }
                reader.push(&buffer[..n]);
                let Ok(received) = handle_frames(&mut reader, &mut stream).await else {
                    return End::Disconnected;
                };
                for r in received {
                    if let Received::Close(code) = r {
                        return if code == INVALID_CLIENT_ID { End::InvalidClientId } else { End::Disconnected };
                    }
                }
            }
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

    use super::*;

    async fn read_frame(
        server: &mut NamedPipeServer,
        reader: &mut FrameReader,
    ) -> (Option<Opcode>, Value) {
        let mut buffer = vec![0u8; 4096];
        loop {
            if let Some(frame) = reader.next_frame().unwrap() {
                return frame;
            }
            let n = server.read(&mut buffer).await.unwrap();
            assert!(n > 0, "client closed");
            reader.push(&buffer[..n]);
        }
    }

    /// A fake Discord on a named pipe with a test prefix.
    #[tokio::test(flavor = "multi_thread")]
    async fn handshake_activity_and_disconnect() {
        let prefix = format!("tracker-test-ipc-{}-", std::process::id());
        let mut server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(socket_name(&prefix, 0))
            .unwrap();

        let (presence, task) = Presence::new("12345".into(), prefix);
        let task = tokio::spawn(task);
        let activity = json!({ "type": 3, "details": "Toradora!" });
        presence.set(true, Some(activity.clone()));

        server.connect().await.unwrap();
        let mut reader = FrameReader::default();
        let (opcode, handshake) = read_frame(&mut server, &mut reader).await;
        assert_eq!(opcode, Some(Opcode::Handshake));
        assert_eq!(handshake, json!({ "v": 1, "client_id": "12345" }));

        // Ping before READY is answered.
        server
            .write_all(&encode(Opcode::Ping, &json!({ "x": 1 })))
            .await
            .unwrap();
        let (opcode, pong) = read_frame(&mut server, &mut reader).await;
        assert_eq!(opcode, Some(Opcode::Pong));
        assert_eq!(pong, json!({ "x": 1 }));

        server
            .write_all(&encode(
                Opcode::Frame,
                &json!({ "cmd": "DISPATCH", "evt": "READY" }),
            ))
            .await
            .unwrap();
        let (_, command) = read_frame(&mut server, &mut reader).await;
        assert_eq!(command["cmd"], "SET_ACTIVITY");
        assert_eq!(command["args"]["activity"], activity);
        assert_eq!(command["args"]["pid"], std::process::id());
        assert_eq!(presence.status(), PresenceStatus::Connected);

        // Clearing sends SET_ACTIVITY without an activity.
        presence.set(true, None);
        let (_, command) = read_frame(&mut server, &mut reader).await;
        assert!(command["args"].get("activity").is_none());

        // Disabling closes the pipe.
        presence.set(false, None);
        let mut buffer = [0u8; 16];
        let n = server.read(&mut buffer).await.unwrap_or(0);
        assert_eq!(n, 0);

        drop(presence);
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn invalid_client_id_stops_retrying() {
        let prefix = format!("tracker-test-ipc-bad-{}-", std::process::id());
        let mut server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(socket_name(&prefix, 0))
            .unwrap();
        let (presence, task) = Presence::new("0".into(), prefix);
        tokio::spawn(task);
        presence.set(true, None);
        server.connect().await.unwrap();
        let mut reader = FrameReader::default();
        read_frame(&mut server, &mut reader).await;
        server
            .write_all(&encode(
                Opcode::Close,
                &json!({ "code": 4000, "message": "Invalid Client ID" }),
            ))
            .await
            .unwrap();
        for _ in 0..50 {
            if presence.status() == PresenceStatus::InvalidClientId {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("status: {:?}", presence.status());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn keeps_trying_while_discord_is_not_running() {
        let prefix = format!("tracker-test-ipc-none-{}-", std::process::id());
        let (presence, task) = Presence::new("1".into(), prefix);
        tokio::spawn(task);
        presence.set(true, None);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(presence.status(), PresenceStatus::Connecting);
        presence.set(false, None);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(presence.status(), PresenceStatus::Disabled);
    }
}
