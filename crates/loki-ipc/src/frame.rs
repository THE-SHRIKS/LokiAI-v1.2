//! Length-prefixed frame codec: `[u32 LE payload length][JSON payload]`.
//!
//! Generic over `AsyncRead`/`AsyncWrite` rather than tied to the named-pipe
//! types directly, so unit tests can drive it over an in-memory
//! [`tokio::io::duplex`] pipe without needing a real OS named pipe.
//!
//! Every rejection path here returns a typed [`FrameError`] — never a
//! panic — per the task packet's "malformed frame handling (rejected, not a
//! panic)" requirement.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::protocol::WireMessage;

/// Default maximum frame payload size: 4 MiB. Generous enough for a large
/// handshake/capability set or a batch of stream items, small enough that a
/// malformed or hostile length prefix cannot make a peer allocate an
/// unbounded buffer before the real (small) payload arrives.
pub const DEFAULT_MAX_FRAME_BYTES: u32 = 4 * 1024 * 1024;

/// A frame could not be read or written.
#[derive(Debug)]
pub enum FrameError {
    /// The underlying transport failed (including a truncated frame — EOF
    /// after the length prefix but before the full payload arrived).
    Io(std::io::Error),
    /// The declared payload length exceeds the configured maximum. The
    /// payload is never read/allocated in this case.
    TooLarge { len: u32, max: u32 },
    /// The payload's bytes were not valid UTF-8 JSON, or did not match any
    /// [`WireMessage`] variant.
    Malformed(serde_json::Error),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::Io(err) => write!(f, "frame I/O error: {err}"),
            FrameError::TooLarge { len, max } => {
                write!(f, "frame too large: {len} bytes (maximum {max})")
            }
            FrameError::Malformed(err) => write!(f, "malformed frame payload: {err}"),
        }
    }
}

impl std::error::Error for FrameError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FrameError::Io(err) => Some(err),
            FrameError::TooLarge { .. } => None,
            FrameError::Malformed(err) => Some(err),
        }
    }
}

/// Serializes and writes one frame. `Err` means nothing (or only a partial
/// frame) reached the peer; the caller should treat the connection as dead
/// rather than retry a partial write.
pub async fn write_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    message: &WireMessage,
) -> Result<(), FrameError> {
    let payload = serde_json::to_vec(message).map_err(FrameError::Malformed)?;
    let len = u32::try_from(payload.len()).map_err(|_| FrameError::TooLarge {
        len: u32::MAX,
        max: DEFAULT_MAX_FRAME_BYTES,
    })?;
    if len > DEFAULT_MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge {
            len,
            max: DEFAULT_MAX_FRAME_BYTES,
        });
    }
    writer
        .write_all(&len.to_le_bytes())
        .await
        .map_err(FrameError::Io)?;
    writer.write_all(&payload).await.map_err(FrameError::Io)?;
    Ok(())
}

/// Reads one frame, honoring `max_len` as the largest acceptable payload
/// size.
///
/// Returns `Ok(None)` exactly when the peer closed the connection cleanly
/// *between* frames (zero bytes read while starting a new length prefix) —
/// this is a normal disconnect, not a protocol error. Any other short read
/// (a peer that closes mid-length-prefix or mid-payload) is a genuine
/// truncated frame and is reported as `Err(FrameError::Io(..))`, matching
/// this crate's "malformed frame handling (rejected, not a panic)"
/// requirement without conflating it with an ordinary disconnect.
pub async fn read_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
    max_len: u32,
) -> Result<Option<WireMessage>, FrameError> {
    let Some(len) = read_length_prefix(reader).await? else {
        return Ok(None);
    };
    if len > max_len {
        return Err(FrameError::TooLarge { len, max: max_len });
    }
    let mut payload = vec![0u8; len as usize];
    reader
        .read_exact(&mut payload)
        .await
        .map_err(FrameError::Io)?;
    let message = serde_json::from_slice(&payload).map_err(FrameError::Malformed)?;
    Ok(Some(message))
}

/// Reads the 4-byte little-endian length prefix, distinguishing "clean EOF
/// before any byte of a new frame arrived" (`Ok(None)`) from "EOF partway
/// through the length prefix" (`Err`, a truncated frame).
async fn read_length_prefix<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<Option<u32>, FrameError> {
    let mut buf = [0u8; 4];
    let mut filled = 0usize;
    while filled < buf.len() {
        let n = reader
            .read(&mut buf[filled..])
            .await
            .map_err(FrameError::Io)?;
        if n == 0 {
            return if filled == 0 {
                Ok(None)
            } else {
                Err(FrameError::Io(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "connection closed mid-frame-length",
                )))
            };
        }
        filled += n;
    }
    Ok(Some(u32::from_le_bytes(buf)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Method, PingParams};
    use uuid::Uuid;

    fn sample_message() -> WireMessage {
        WireMessage::Call {
            correlation_id: Uuid::nil(),
            deadline_unix_millis: 123,
            method: Method::Ping(PingParams {
                payload: "hello".to_string(),
            }),
        }
    }

    #[tokio::test]
    async fn write_then_read_round_trips_one_frame() {
        let (mut client, mut server) = tokio::io::duplex(4096);
        write_frame(&mut client, &sample_message()).await.unwrap();
        drop(client);

        let decoded = read_frame(&mut server, DEFAULT_MAX_FRAME_BYTES)
            .await
            .unwrap()
            .expect("expected one frame, got clean EOF");
        match decoded {
            WireMessage::Call {
                method: Method::Ping(params),
                ..
            } => assert_eq!(params.payload, "hello"),
            other => panic!("unexpected message: {other:?}"),
        }
    }

    #[tokio::test]
    async fn multiple_frames_on_one_stream_are_read_in_order() {
        let (mut client, mut server) = tokio::io::duplex(8192);
        for i in 0..5u32 {
            let msg = WireMessage::CancelCall {
                correlation_id: Uuid::from_u128(i as u128),
            };
            write_frame(&mut client, &msg).await.unwrap();
        }
        drop(client);

        for i in 0..5u32 {
            let decoded = read_frame(&mut server, DEFAULT_MAX_FRAME_BYTES)
                .await
                .unwrap()
                .expect("expected a frame");
            match decoded {
                WireMessage::CancelCall { correlation_id } => {
                    assert_eq!(correlation_id, Uuid::from_u128(i as u128));
                }
                other => panic!("unexpected message: {other:?}"),
            }
        }
        assert!(
            read_frame(&mut server, DEFAULT_MAX_FRAME_BYTES)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn clean_close_between_frames_is_none_not_an_error() {
        let (client, mut server) = tokio::io::duplex(64);
        drop(client);
        let result = read_frame(&mut server, DEFAULT_MAX_FRAME_BYTES).await;
        assert!(matches!(result, Ok(None)));
    }

    #[tokio::test]
    async fn truncated_length_prefix_is_a_clear_error_not_a_panic() {
        let (mut client, mut server) = tokio::io::duplex(64);
        client.write_all(&[1, 2]).await.unwrap(); // only 2 of 4 length bytes
        drop(client);

        let result = read_frame(&mut server, DEFAULT_MAX_FRAME_BYTES).await;
        assert!(matches!(result, Err(FrameError::Io(_))));
    }

    #[tokio::test]
    async fn truncated_payload_is_a_clear_error_not_a_panic() {
        let (mut client, mut server) = tokio::io::duplex(64);
        client.write_all(&100u32.to_le_bytes()).await.unwrap(); // claims 100 bytes
        client.write_all(b"short").await.unwrap(); // only sends 5
        drop(client);

        let result = read_frame(&mut server, DEFAULT_MAX_FRAME_BYTES).await;
        assert!(matches!(result, Err(FrameError::Io(_))));
    }

    #[tokio::test]
    async fn oversized_length_prefix_is_rejected_without_allocating() {
        let (mut client, mut server) = tokio::io::duplex(64);
        // Claims a 1 GiB payload but never sends it — if `read_frame` tried
        // to allocate/read that much, this test would hang or exhaust
        // memory instead of returning promptly.
        client
            .write_all(&(1024u32 * 1024 * 1024).to_le_bytes())
            .await
            .unwrap();

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            read_frame(&mut server, DEFAULT_MAX_FRAME_BYTES),
        )
        .await
        .expect("read_frame must reject an oversized length prefix promptly, not hang");

        assert!(matches!(result, Err(FrameError::TooLarge { .. })));
    }

    #[tokio::test]
    async fn invalid_json_payload_is_a_clear_error_not_a_panic() {
        let (mut client, mut server) = tokio::io::duplex(64);
        let payload = b"not json";
        client
            .write_all(&(payload.len() as u32).to_le_bytes())
            .await
            .unwrap();
        client.write_all(payload).await.unwrap();
        drop(client);

        let result = read_frame(&mut server, DEFAULT_MAX_FRAME_BYTES).await;
        assert!(matches!(result, Err(FrameError::Malformed(_))));
    }

    #[tokio::test]
    async fn valid_json_that_is_not_a_wire_message_is_a_clear_error() {
        let (mut client, mut server) = tokio::io::duplex(64);
        let payload = br#"{"totally":"unrelated"}"#;
        client
            .write_all(&(payload.len() as u32).to_le_bytes())
            .await
            .unwrap();
        client.write_all(payload).await.unwrap();
        drop(client);

        let result = read_frame(&mut server, DEFAULT_MAX_FRAME_BYTES).await;
        assert!(matches!(result, Err(FrameError::Malformed(_))));
    }

    #[tokio::test]
    async fn write_frame_rejects_a_payload_larger_than_the_default_maximum() {
        // Build a message whose serialized JSON exceeds the default limit.
        let huge_payload = "x".repeat(DEFAULT_MAX_FRAME_BYTES as usize + 1);
        let msg = WireMessage::Call {
            correlation_id: Uuid::nil(),
            deadline_unix_millis: 0,
            method: Method::Ping(PingParams {
                payload: huge_payload,
            }),
        };
        let (mut client, _server) = tokio::io::duplex(64);
        let result = write_frame(&mut client, &msg).await;
        assert!(matches!(result, Err(FrameError::TooLarge { .. })));
    }
}
