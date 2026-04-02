# Component Interaction Diagram

```mermaid
graph LR
    subgraph "Rust Process"
        UI[ChatApp<br/>egui UI]
        REC[AudioRecorder<br/>cpal]
        STT_C[STTModel<br/>Client]
        LLM_C[AIModel<br/>Client]
        TTS_C[TTSModel<br/>Client]
        PLAY[Audio Playback<br/>rodio Sink]
        BE[AIVAEngine<br/>Process Manager]
    end

    subgraph "Python Process (AIVAEngine)"
        STT_S[STTService<br/>faster-whisper]
        LLM_S[LLMService<br/>llama-cpp-python]
        TTS_S[TTSService<br/>Piper TTS]
    end

    UI -->|Start/Stop| REC
    REC -->|WAV bytes| STT_C
    STT_C -->|base64 JSON| BE
    BE -->|stdin| STT_S
    STT_S -->|stdout| BE
    BE -->|transcription| STT_C
    STT_C -->|text| UI

    UI -->|user message| LLM_C
    LLM_C -->|JSON| BE
    BE -->|stdin| LLM_S
    LLM_S -->|streaming tokens| BE
    BE -->|tokens| LLM_C
    LLM_C -->|response| UI

    UI -->|response text| TTS_C
    TTS_C -->|JSON| BE
    BE -->|stdin| TTS_S
    TTS_S -->|audio chunks| BE
    BE -->|WAV chunks| TTS_C
    TTS_C -->|decoded audio| PLAY

    style UI fill:#2d5a27,color:#fff
    style BE fill:#5a3a1a,color:#fff
    style STT_S fill:#1a3a5c,color:#fff
    style LLM_S fill:#1a3a5c,color:#fff
    style TTS_S fill:#1a3a5c,color:#fff
```

All three service clients (`STTModel`, `AIModel`, `TTSModel`) share a single `AIVAEngine` process via `Arc<Mutex<AIVAEngine>>`. Requests are serialized through the mutex.
