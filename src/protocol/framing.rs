// Length-prefixed message framing
//
// Message format: [4-byte length (big-endian)][JSON payload]

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::{Message, MAX_MESSAGE_SIZE};

/// Read a length-prefixed message from an async reader
pub async fn read_message<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Message> {
    // Read 4-byte length prefix
    let length = reader
        .read_u32()
        .await
        .context("Failed to read message length")?;

    // Validate length
    if length == 0 {
        bail!("Message length cannot be zero");
    }

    if length as usize > MAX_MESSAGE_SIZE {
        bail!(
            "Message too large: {} bytes (max: {})",
            length,
            MAX_MESSAGE_SIZE
        );
    }

    // Read message payload
    let mut buffer = vec![0u8; length as usize];
    reader
        .read_exact(&mut buffer)
        .await
        .context("Failed to read message payload")?;

    // Deserialize JSON
    let message: Message =
        serde_json::from_slice(&buffer).context("Failed to deserialize message")?;

    Ok(message)
}

/// Write a length-prefixed message to an async writer
pub async fn write_message<W: AsyncWrite + Unpin>(writer: &mut W, message: &Message) -> Result<()> {
    // Serialize to JSON
    let json = serde_json::to_vec(message).context("Failed to serialize message")?;

    // Validate length
    if json.len() > MAX_MESSAGE_SIZE {
        bail!(
            "Message too large: {} bytes (max: {})",
            json.len(),
            MAX_MESSAGE_SIZE
        );
    }

    // Write length prefix (big-endian)
    writer
        .write_u32(json.len() as u32)
        .await
        .context("Failed to write message length")?;

    // Write payload
    writer
        .write_all(&json)
        .await
        .context("Failed to write message payload")?;

    // Flush to ensure message is sent
    writer.flush().await.context("Failed to flush writer")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Method;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn test_read_write_message() {
        let msg = Message::request(
            Method::Ping,
            serde_json::json!({}),
        );

        // Write to buffer
        let mut buffer = Vec::new();
        write_message(&mut buffer, &msg).await.unwrap();

        // Read from buffer
        let mut reader = BufReader::new(&buffer[..]);
        let decoded = read_message(&mut reader).await.unwrap();

        match decoded {
            Message::Request(req) => {
                assert!(matches!(req.method, Method::Ping));
            }
            _ => panic!("Expected Request"),
        }
    }

    #[tokio::test]
    async fn test_message_too_large() {
        // Create a message that's too large
        let large_data = "x".repeat(MAX_MESSAGE_SIZE + 1);
        let msg = Message::request(
            Method::TestStart,
            serde_json::json!({"data": large_data}),
        );

        let mut buffer = Vec::new();
        let result = write_message(&mut buffer, &msg).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }
}
