# ADR-004: Stdin/Stdout IPC Over HTTP or gRPC

## Status: Accepted

## Context
The Rust frontend needs to communicate with the Python ML backend. Options considered:
1. HTTP/REST with a local server
2. gRPC with protobuf definitions
3. WebSocket for bidirectional streaming
4. stdin/stdout with newline-delimited JSON

## Decision
Stdin/stdout with newline-delimited JSON was chosen.

The Python backend runs as a child process of the Rust frontend. Communication happens through the child's stdin (commands) and stdout (responses). Each message is a single JSON object on one line.

## Consequences
- **Easier:** Zero configuration. No ports, no service discovery, no firewall prompts on Windows.
- **Easier:** Process lifecycle. The child process starts when the parent starts and is killed when the parent exits (`Drop` implementation). No orphan processes.
- **Easier:** Streaming. Token-by-token LLM output and chunk-by-chunk TTS audio are naturally expressed as a sequence of JSON lines.
- **Harder:** No concurrent requests. The protocol is inherently serial (one request, read responses until done, next request). This is acceptable because the pipeline is sequential (STT -> LLM -> TTS).
- **Harder:** Debugging. There's no curl-able endpoint. Debugging requires reading the JSON protocol manually or running the Python server standalone.
- **Trade-off:** If the system needed to support multiple frontends or remote access, HTTP would be necessary. For a single-user desktop app, stdio is simpler and more reliable.
