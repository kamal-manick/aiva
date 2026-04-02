# ADR-005: Single Combined Engine Over Separate Processes

## Status: Accepted

## Context
The initial architecture used three separate Python processes: `llm_server.py`, `stt_server.py`, and `tts_server.py`. Each handled one inference service and communicated via its own stdin/stdout pipe.

## Decision
The three processes were consolidated into a single `aiva_engine.py` that handles all three services.

## Consequences
- **Easier:** Memory efficiency. One Python runtime instead of three. Shared numpy, ONNX Runtime, and other heavy libraries loaded once.
- **Easier:** Process management. One child process to spawn, monitor, and kill instead of three.
- **Easier:** Startup time. One process initialization instead of three parallel startups competing for CPU and memory.
- **Harder:** A crash in any service takes down all three. With separate processes, STT and TTS could continue if the LLM crashed.
- **Harder:** No parallelism between services. With separate processes, STT could theoretically run while TTS is playing. In practice, the pipeline is sequential, so this doesn't matter.
- **Trade-off:** The separate server scripts (`llm_server.py`, `stt_server.py`, `tts_server.py`) are preserved in the codebase for development and testing, but the combined engine is the production deployment target.
