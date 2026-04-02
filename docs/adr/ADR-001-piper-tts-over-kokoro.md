# ADR-001: Piper TTS Over Kokoro TTS

## Status: Accepted

## Context
The voice assistant requires a text-to-speech engine that runs locally on CPU. Two candidates were evaluated: Piper TTS (ONNX Runtime, VITS-based) and Kokoro TTS (newer, higher-quality prosody). The system has a hard latency constraint of <10s end-to-end from speech input to audio output.

## Decision
Piper TTS was chosen over Kokoro TTS.

Kokoro produces noticeably better prosody and more natural-sounding speech. However, Piper's synthesis speed enables near-real-time sentence-level streaming -- the first audio chunk begins playing while later sentences are still being synthesized. This streaming capability is critical for perceived latency.

In a voice assistant, users judge responsiveness by time-to-first-audio, not by voice quality. A fast, decent-sounding response feels better than a slow, beautiful one.

## Consequences
- **Easier:** Achieving the <10s latency target. Piper's sentence-level streaming maps cleanly onto the chunked audio delivery protocol.
- **Easier:** Multi-speaker support via Piper's VITS multi-speaker models (configurable via `speaker_id` in settings.json).
- **Harder:** Voice quality is noticeably more synthetic than Kokoro. Users who prioritize naturalness over speed may prefer Kokoro.
- **Trade-off:** If latency constraints relax (e.g., GPU deployment), Kokoro becomes the better choice. The TTS service interface is abstracted enough to swap implementations.
