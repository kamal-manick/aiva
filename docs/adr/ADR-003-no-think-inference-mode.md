# ADR-003: /no_think Inference Mode

## Status: Accepted

## Context
Qwen3 supports a "thinking" mode where the model generates internal reasoning tokens (`<think>...</think>`) before producing the visible response. This improves response quality for complex questions but adds significant latency -- the thinking tokens consume time and compute before the user sees any output.

## Decision
The system prompt includes `/no_think` to disable the thinking mode entirely.

In a voice assistant, the user is waiting in silence while the model generates. Every second of delay is felt. Disabling thinking mode means:
- Time-to-first-token is faster (no hidden reasoning overhead)
- The streaming display shows output immediately
- TTS can begin synthesizing the first sentence sooner

The thinking tokens are also filtered out during streaming as a safety net, in case the model generates them despite `/no_think`.

## Consequences
- **Easier:** Meeting the latency target. First visible token appears significantly faster.
- **Easier:** Streaming UX -- the user sees text appearing immediately rather than waiting through an invisible thinking phase.
- **Harder:** Complex multi-step questions may get shallower answers. The model doesn't have the opportunity to "reason through" a problem before responding.
- **Trade-off:** For a voice assistant handling conversational queries, the speed gain outweighs the quality loss. For a code assistant or analytical tool, thinking mode would be preferred.
