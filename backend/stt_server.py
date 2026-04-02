#!/usr/bin/env python3
"""
STT Server for Rust Voice Assistant
Uses faster-whisper for speech-to-text transcription
Communicates via stdin/stdout JSON protocol
"""

import sys
import json
import base64
import tempfile
import os
from pathlib import Path

# Set HuggingFace cache to local models folder before importing
SCRIPT_DIR = Path(__file__).parent.parent
MODELS_DIR = SCRIPT_DIR / "models"
HF_CACHE_DIR = MODELS_DIR / "hf_cache"
HF_CACHE_DIR.mkdir(parents=True, exist_ok=True)

os.environ["HF_HOME"] = str(HF_CACHE_DIR)
os.environ["HF_HUB_CACHE"] = str(HF_CACHE_DIR)
os.environ["HF_HUB_DISABLE_SYMLINKS_WARNING"] = "1"

from faster_whisper import WhisperModel

# Model size: tiny, base, small, medium, large-v2, large-v3
MODEL_SIZE = "base"


def send_response(response_type: str, data: dict):
    """Send JSON response to stdout."""
    response = {"type": response_type, **data}
    print(json.dumps(response), flush=True)


def main():
    send_response("status", {"message": "Loading Whisper model..."})

    try:
        # Use CPU with int8 quantization for efficiency
        model = WhisperModel(MODEL_SIZE, device="cpu", compute_type="int8")
        send_response("status", {"message": f"Whisper {MODEL_SIZE} model loaded successfully"})
    except Exception as e:
        send_response("error", {"message": f"Failed to load Whisper model: {e}"})
        sys.exit(1)

    # Main loop - read JSON requests from stdin
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue

        try:
            request = json.loads(line)
        except json.JSONDecodeError as e:
            send_response("error", {"message": f"Invalid JSON: {e}"})
            continue

        cmd = request.get("cmd")

        if cmd == "quit":
            send_response("status", {"message": "Shutting down"})
            break

        elif cmd == "transcribe":
            # Audio data is base64-encoded WAV bytes
            audio_b64 = request.get("audio")
            if not audio_b64:
                send_response("error", {"message": "No audio data provided"})
                continue

            try:
                # Decode base64 audio
                audio_bytes = base64.b64decode(audio_b64)

                # Write to temp file (faster-whisper needs a file path)
                with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
                    f.write(audio_bytes)
                    temp_path = f.name

                try:
                    # Transcribe
                    segments, info = model.transcribe(
                        temp_path,
                        beam_size=5,
                        language="en",  # Set to None for auto-detection
                        vad_filter=True,  # Filter out silence
                    )

                    # Collect all segments
                    text_parts = []
                    for segment in segments:
                        text_parts.append(segment.text.strip())

                    full_text = " ".join(text_parts).strip()

                    send_response("transcription", {
                        "text": full_text,
                        "language": info.language,
                        "language_probability": info.language_probability,
                    })

                finally:
                    # Clean up temp file
                    os.unlink(temp_path)

            except Exception as e:
                send_response("error", {"message": f"Transcription failed: {e}"})

        elif cmd == "transcribe_file":
            # Transcribe from a file path (for testing)
            file_path = request.get("path")
            if not file_path or not Path(file_path).exists():
                send_response("error", {"message": f"File not found: {file_path}"})
                continue

            try:
                segments, info = model.transcribe(
                    file_path,
                    beam_size=5,
                    language="en",
                    vad_filter=True,
                )

                text_parts = []
                for segment in segments:
                    text_parts.append(segment.text.strip())

                full_text = " ".join(text_parts).strip()

                send_response("transcription", {
                    "text": full_text,
                    "language": info.language,
                    "language_probability": info.language_probability,
                })

            except Exception as e:
                send_response("error", {"message": f"Transcription failed: {e}"})

        elif cmd == "ping":
            send_response("pong", {})

        else:
            send_response("error", {"message": f"Unknown command: {cmd}"})


if __name__ == "__main__":
    main()
