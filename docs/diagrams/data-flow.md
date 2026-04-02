# Data Flow Diagram

```mermaid
sequenceDiagram
    participant User
    participant UI as ChatApp (Rust)
    participant REC as AudioRecorder
    participant ENG as AIVAEngine (Python)
    participant PLAY as Audio Playback

    Note over User,PLAY: Voice Input Flow
    User->>UI: Click Mic
    UI->>REC: start_recording()
    REC-->>REC: Capture audio (cpal)
    User->>UI: Click Stop
    UI->>REC: stop_recording()
    REC-->>UI: WAV bytes (16kHz mono)

    Note over User,PLAY: Speech-to-Text
    UI->>ENG: {"cmd":"transcribe", "audio":"<base64>"}
    ENG-->>ENG: faster-whisper inference
    ENG->>UI: {"type":"transcription", "text":"Hello"}

    Note over User,PLAY: LLM Reasoning (Streaming)
    UI->>ENG: {"cmd":"generate", "messages":[...]}
    ENG->>UI: {"type":"generation_start"}
    loop For each token
        ENG->>UI: {"type":"token", "token":"Hi"}
        UI-->>UI: Append to streaming display
    end
    ENG->>UI: {"type":"generation_end", "full_response":"Hi there!"}

    Note over User,PLAY: Text-to-Speech (Streaming)
    UI->>ENG: {"cmd":"synthesize", "text":"Hi there!", "streaming":true}
    loop For each sentence chunk
        ENG->>UI: {"type":"audio_chunk", "audio":"<base64 WAV>"}
        UI->>PLAY: Decode + queue in rodio Sink
        PLAY-->>User: Audio output begins
    end
    ENG->>UI: {"type":"audio_done"}
    PLAY-->>User: Remaining audio plays out
```

Key insight: streaming at every stage means the user hears audio before the full response is generated. The LLM's first sentence triggers TTS synthesis immediately, and playback begins as soon as that first chunk is ready.
