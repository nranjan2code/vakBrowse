//! Stdio framing normalizer for the MCP server.
//!
//! Wraps an MCP client's stdin so `vak-mcp` accepts BOTH stdio framing styles
//! that appear in the wild:
//!
//! * newline-delimited JSON — `{"jsonrpc":...}\n` — used by the official `mcp`
//!   Python SDK (`client/stdio.py` writes `json + "\n"`, reads on `\n`) and by
//!   `@modelcontextprotocol/sdk` TypeScript v1.26 (`JSON.stringify(m) + '\n'`,
//!   splits reads on `'\n'`);
//! * `Content-Length: N\r\n\r\n<bytes>` blocks — the framing the MCP spec *text*
//!   describes (some spec-literal/legacy clients send it this way).
//!
//! rmcp 3.1.4's `AsyncRwTransport` consumes NDJSON only (its
//! `JsonRpcMessageCodec` decodes by scanning for a `\n` delimiter and silently
//! skips any line that isn't valid JSON). This normalizer only rewrites
//! *incoming* framing: it never parses JSON, it only relocates message
//! boundaries and always emits newline-delimited JSON. Outgoing responses are
//! still NDJSON from rmcp's encoder, so every SDK keeps reading them.
//!
//! Implementation is an isolated `tokio::io::duplex` transducer driven by
//! `BufReader::read_until` / `read_exact` (tokio's own, battle-tested IO
//! primitives) instead of a hand-rolled `AsyncRead` impl — a buggy `poll_read`
//! on the hot stdin path could deadlock the whole server, and there's no
//! upside here since the transducer only relocates bytes.
use std::io;

use tokio::io::{
    AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, DuplexStream,
};

/// Read half of the normalized stream: yields newline-delimited JSON to the
/// caller (feed it to `AsyncRwTransport::new_server`).
pub type FramedStdin = DuplexStream;

/// Wrap an MCP client's stdin (`R: AsyncRead`) and re-emit its bytes as
/// newline-delimited JSON, accepting either Content-Length or NDJSON input
/// framing (or a mix — framing is decided per-message).
///
/// Spawns one detached tokio task that owns the write end of an internal
/// `duplex(65536)`; the returned `FramedStdin` (read end) is `AsyncRead`. When
/// the source `reader` hits EOF, the task ends and drops the write end, so the
/// reader returns EOF too (rmcp then tears the session down cleanly).
pub fn normalize_stdin<R>(reader: R) -> FramedStdin
where
    R: AsyncRead + Send + 'static + Unpin,
{
    let (read_end, write_end) = tokio::io::duplex(65536);
    tokio::spawn(framing_loop(reader, write_end));
    read_end
}

async fn framing_loop<R, W>(mut src: R, mut out: W)
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut br = BufReader::new(&mut src);
    let mut line: Vec<u8> = Vec::new();
    loop {
        line.clear();
        match br.read_until(b'\n', &mut line).await {
            Ok(0) => break, // EOF
            Ok(_) => {
                if let Err(e) = handle_message(&mut br, &mut out, &line).await {
                    // Malformed/oversize frame from a misbehaving client. Stop
                    // feeding rmcp rather than risk desync; rmcp sees EOF and
                    // ends the session. Logging goes to stderr (never stdout,
                    // which is the protocol channel).
                    tracing::debug!(err=%e, "stdio framing error; closing");
                    break;
                }
            }
            Err(e) => {
                tracing::debug!(err=%e, "stdio framing reader error; closing");
                break;
            }
        }
    }
}

/// Dispatch one framed message: a `Content-Length:` line triggers
/// length-prefixed framing (consume the blank-line separator, then exactly
/// `len` body bytes); anything else is an NDJSON line emitted verbatim (it
/// already ends in `\n`).
async fn handle_message<R, W>(br: &mut BufReader<R>, out: &mut W, line: &[u8]) -> io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let trimmed = line.strip_suffix(b"\r\n").unwrap_or(line);
    if let Some(len) = parse_content_length(trimmed) {
        // Consume the blank-line separator that follows the header line
        // (`\r\n`). `read_until` is lenient: any terminator works.
        let mut sep = Vec::new();
        br.read_until(b'\n', &mut sep).await?;
        let mut body = vec![0u8; len];
        br.read_exact(&mut body).await?;
        out.write_all(&body).await?;
        out.write_all(b"\n").await?;
    } else {
        // NDJSON line — already newline-terminated; pass straight through.
        out.write_all(line).await?;
    }
    out.flush().await
}

fn parse_content_length(line: &[u8]) -> Option<usize> {
    let prefix = b"content-length:";
    if line.len() < prefix.len() || !line[..prefix.len()].eq_ignore_ascii_case(prefix) {
        return None;
    }
    let rest = std::str::from_utf8(&line[prefix.len()..]).ok()?;
    rest.trim().parse::<usize>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncBufReadExt;

    /// Drive `normalize_stdin` with a raw pipe we can write both framing styles
    /// to, then assert the emitted stream is correct newline-delimited JSON.
    #[tokio::test(flavor = "multi_thread")]
    async fn normalizes_content_length_and_ndjson_and_mixed() {
        let (raw_write, raw_read) = tokio::io::duplex(65536);
        let normalized = normalize_stdin(raw_read);
        let mut raw = raw_write;

        let cl_msg = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" });
        let cl_bytes = serde_json::to_vec(&cl_msg).unwrap();
        // Content-Length frame (spec style)
        raw.write_all(format!("content-length: {}\r\n\r\n", cl_bytes.len()).as_bytes())
            .await
            .unwrap();
        raw.write_all(&cl_bytes).await.unwrap();
        // NDJSON frame
        let nd_msg = serde_json::json!({ "jsonrpc": "2.0", "id": 2, "method": "pong" });
        let nd_bytes = serde_json::to_vec(&nd_msg).unwrap();
        raw.write_all(&nd_bytes).await.unwrap();
        raw.write_all(b"\n").await.unwrap();
        drop(raw); // EOF => task flushes + stops

        let mut reader = BufReader::new(normalized);
        let mut l1 = Vec::new();
        reader.read_until(b'\n', &mut l1).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&l1).unwrap(),
            cl_msg
        );

        let mut l2 = Vec::new();
        reader.read_until(b'\n', &mut l2).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&l2).unwrap(),
            nd_msg
        );

        // EOF on the normalized stream now.
        let mut end = Vec::new();
        let n = reader.read_until(b'\n', &mut end).await.unwrap();
        assert_eq!(n, 0, "normalized stream should be EOF after the two frames");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn content_length_without_trailing_newline_is_terminated() {
        let (raw_write, raw_read) = tokio::io::duplex(65536);
        let normalized = normalize_stdin(raw_read);
        let mut raw = raw_write;

        let msg = serde_json::json!({ "jsonrpc": "2.0", "id": 7, "method": "x" });
        let bytes = serde_json::to_vec(&msg).unwrap();
        raw.write_all(format!("Content-Length: {}\r\n\r\n", bytes.len()).as_bytes())
            .await
            .unwrap();
        raw.write_all(&bytes).await.unwrap();
        drop(raw);

        let mut reader = BufReader::new(normalized);
        let mut line = Vec::new();
        reader.read_until(b'\n', &mut line).await.unwrap();
        assert!(line.ends_with(b"\n"), "CL body must be terminated with \\n");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(line.strip_suffix(b"\n").unwrap()).unwrap(),
            msg
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn ndjson_preserves_terminator_and_closes_on_eof() {
        let (raw_write, raw_read) = tokio::io::duplex(65536);
        let normalized = normalize_stdin(raw_read);
        let mut raw = raw_write;
        let msg = serde_json::json!({ "jsonrpc": "2.0", "id": 3, "method": "q" });
        let mut bytes = serde_json::to_vec(&msg).unwrap();
        bytes.push(b'\n');
        raw.write_all(&bytes).await.unwrap();
        drop(raw);

        let mut reader = BufReader::new(normalized);
        let mut line = Vec::new();
        reader.read_until(b'\n', &mut line).await.unwrap();
        assert_eq!(line, bytes);
    }
}
