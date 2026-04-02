# ADR-006: Rust Frontend with Python Backend

## Status: Accepted

## Context
The application needs a native desktop GUI with audio I/O (frontend) and ML model inference (backend). The question is which language handles which responsibility.

Options:
1. All Python (e.g., PyQt + inference)
2. All Rust (e.g., egui + ort bindings for ONNX)
3. Rust frontend + Python backend

## Decision
Rust handles the frontend (GUI, audio capture, audio playback, process management). Python handles the backend (LLM, STT, TTS inference).

## Consequences
- **Easier:** Small, fast frontend binary. The Rust exe is ~5MB and starts instantly. No Python runtime needed on the frontend side.
- **Easier:** ML ecosystem access. llama-cpp-python, faster-whisper, and piper-tts are mature, well-optimized Python libraries. Equivalent Rust bindings are immature or non-existent.
- **Easier:** Rapid model experimentation. Swapping models in Python is trivial -- change an import and a path. In Rust, it would require recompilation and potentially binding changes.
- **Harder:** Two-language codebase. Contributors need both Rust and Python knowledge.
- **Harder:** Packaging. The Python backend must be bundled via PyInstaller for distribution, adding build complexity.
- **Trade-off:** The IPC overhead of crossing the language boundary (~1ms per message) is negligible compared to inference time (~100ms-5s). The boundary is in the right place -- it separates the latency-sensitive UI work from the compute-heavy inference work.
