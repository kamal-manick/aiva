# ADR-002: Qwen 0.6B as the LLM

## Status: Accepted

## Context
The target deployment is CPU-only devices (no GPU). The LLM must fit in reasonable RAM (<1GB for the model), generate tokens fast enough for conversational use, and ideally support extensibility via tool calling.

Models evaluated: Qwen3 0.6B, Qwen3 1.5B, Phi-3 Mini, TinyLlama 1.1B.

## Decision
Qwen3 0.6B in Q4_K_M GGUF quantization was selected.

- **Size:** ~400MB on disk, fits comfortably in 4GB system RAM alongside the STT and TTS models.
- **Speed:** Generates tokens fast enough for conversational use on a 4-core CPU.
- **Tool calling:** Qwen3 natively supports tool/function calling, enabling future extension of the assistant to perform specific tasks (calendar, file operations, API calls) without changing the inference stack.
- **Quality:** Responses are concise and adequate for a voice assistant. Longer, more nuanced answers would benefit from a larger model, but conciseness is actually desirable for spoken output.

## Consequences
- **Easier:** Deployment on commodity hardware. No GPU required. Fast cold start.
- **Easier:** Future extensibility via tool calling -- the model can be taught to invoke external tools through its native function-calling format.
- **Harder:** Complex reasoning tasks. The 0.6B parameter count limits the model's ability to handle multi-step logic or nuanced questions.
- **Trade-off:** The architecture supports model swapping via `settings.json` -- upgrading to Qwen3 1.5B or larger requires only changing the model path and download URL. No code changes needed.
