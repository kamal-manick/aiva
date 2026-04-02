# ADR-007: Config-Driven Model Selection

## Status: Accepted

## Context
The system uses three ML models (LLM, STT, TTS) that users may want to customize -- different voice, different LLM size, different Whisper model. Hardcoding model paths and parameters in source code would require recompilation for each change.

## Decision
All model paths, download URLs, and inference parameters are externalized to `settings.json`. The application loads this file at startup with sensible defaults as fallback.

Configurable parameters include:
- Model file paths (LLM GGUF, TTS ONNX, STT cache directory)
- Download URLs for auto-download on first run
- TTS voice parameters (speaker ID, speed, noise characteristics)
- LLM context length

## Consequences
- **Easier:** Model swapping without recompilation. Change the GGUF path to use a different LLM. Change the ONNX path to use a different voice.
- **Easier:** Two distribution modes. Bundled distributions include models at the configured paths. Lightweight distributions rely on the download URLs.
- **Easier:** Per-deployment customization. Different speaker_id values select different voices from multi-speaker TTS models.
- **Harder:** Configuration errors (wrong paths, incompatible models) surface at runtime rather than compile time.
- **Trade-off:** A validated schema or config UI would improve the user experience, but plain JSON is sufficient for the current technical audience.
