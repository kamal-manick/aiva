#!/usr/bin/env python3
"""
TTS Server for Rust Voice Assistant
Uses Piper for text-to-speech synthesis
Communicates via stdin/stdout JSON protocol
Supports streaming audio chunks for low-latency playback
"""

import sys
import json
import wave
import io
import base64
import urllib.request
import shutil
from pathlib import Path

import numpy as np

# Paths
SCRIPT_DIR = Path(__file__).parent.parent
SETTINGS_PATH = SCRIPT_DIR / "settings.json"

# Default values (fallback if settings.json is missing or incomplete)
DEFAULTS = {
    "model_path": "models/tts/piper/model.onnx",
    "config_path": "models/tts/piper/model.onnx.json",
    "download_url": "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/libritts_r/medium/en_US-libritts_r-medium.onnx",
    "config_download_url": "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/libritts_r/medium/en_US-libritts_r-medium.onnx.json",
    "speaker_id": 237,
    "length_scale": 0.9,
    "noise_scale": 0.6,
    "noise_w_scale": 0.3
}


def load_settings() -> dict:
    """Load TTS settings from settings.json with fallback to defaults."""
    settings = DEFAULTS.copy()

    if SETTINGS_PATH.exists():
        try:
            with open(SETTINGS_PATH, 'r') as f:
                file_settings = json.load(f)

            # Load model paths
            tts_models = file_settings.get("models", {}).get("tts", {})
            if "model_path" in tts_models:
                settings["model_path"] = tts_models["model_path"]
            if "config_path" in tts_models:
                settings["config_path"] = tts_models["config_path"]
            if "download_url" in tts_models:
                settings["download_url"] = tts_models["download_url"]
            if "config_download_url" in tts_models:
                settings["config_download_url"] = tts_models["config_download_url"]

            # Load TTS settings
            tts_settings = file_settings.get("tts_settings", {})
            if "speaker_id" in tts_settings:
                settings["speaker_id"] = tts_settings["speaker_id"]
            if "length_scale" in tts_settings:
                settings["length_scale"] = tts_settings["length_scale"]
            if "noise_scale" in tts_settings:
                settings["noise_scale"] = tts_settings["noise_scale"]
            if "noise_w_scale" in tts_settings:
                settings["noise_w_scale"] = tts_settings["noise_w_scale"]

        except Exception as e:
            print(f"Warning: Failed to load settings.json: {e}", file=sys.stderr)

    return settings


# Load settings
SETTINGS = load_settings()

# Resolved paths
MODEL_PATH = SCRIPT_DIR / SETTINGS["model_path"]
CONFIG_PATH = SCRIPT_DIR / SETTINGS["config_path"]

# Download URLs
MODEL_URL = SETTINGS["download_url"]
CONFIG_URL = SETTINGS["config_download_url"]

# TTS Settings
TTS_SPEAKER_ID = SETTINGS["speaker_id"]
TTS_LENGTH_SCALE = SETTINGS["length_scale"]
TTS_NOISE_SCALE = SETTINGS["noise_scale"]
TTS_NOISE_W_SCALE = SETTINGS["noise_w_scale"]


def sanitize_text(text: str) -> str:
    """Remove invalid Unicode characters (surrogates, etc.) that can't be encoded."""
    if not isinstance(text, str):
        return text
    return text.encode('utf-8', errors='surrogatepass').decode('utf-8', errors='ignore')


def sanitize_data(data):
    """Recursively sanitize all string values in a data structure."""
    if isinstance(data, str):
        return sanitize_text(data)
    elif isinstance(data, dict):
        return {k: sanitize_data(v) for k, v in data.items()}
    elif isinstance(data, list):
        return [sanitize_data(item) for item in data]
    return data


def send_response(response_type: str, data: dict):
    """Send JSON response to stdout."""
    response = {"type": response_type, **data}
    # Use ensure_ascii=True to escape any non-ASCII chars as \uXXXX sequences
    print(json.dumps(response, ensure_ascii=True), flush=True)


def audio_to_wav_bytes(audio_data: np.ndarray, sample_rate: int) -> bytes:
    """Convert numpy audio array to WAV bytes."""
    buffer = io.BytesIO()

    with wave.open(buffer, "wb") as wav_file:
        wav_file.setnchannels(1)  # Mono
        wav_file.setsampwidth(2)  # 16-bit
        wav_file.setframerate(sample_rate)
        wav_file.writeframes(audio_data.tobytes())

    return buffer.getvalue()


def download_file(url: str, dest_path: Path, desc: str = "file") -> bool:
    """Download a file with progress reporting."""
    try:
        send_response("status", {"message": f"Downloading {desc}..."})
        dest_path.parent.mkdir(parents=True, exist_ok=True)
        temp_path = dest_path.with_suffix('.tmp')

        with urllib.request.urlopen(url) as response:
            total_size = int(response.headers.get('Content-Length', 0))
            downloaded = 0
            chunk_size = 1024 * 1024

            with open(temp_path, 'wb') as f:
                while True:
                    chunk = response.read(chunk_size)
                    if not chunk:
                        break
                    f.write(chunk)
                    downloaded += len(chunk)
                    if total_size > 0:
                        pct = int(downloaded * 100 / total_size)
                        send_response("status", {
                            "message": f"Downloading {desc}: {pct}%"
                        })

        shutil.move(str(temp_path), str(dest_path))
        send_response("status", {"message": f"{desc} downloaded successfully"})
        return True

    except Exception as e:
        send_response("error", {"message": f"Failed to download {desc}: {e}"})
        if temp_path.exists():
            temp_path.unlink()
        return False


def main():
    from piper import PiperVoice
    from piper.voice import SynthesisConfig

    send_response("status", {"message": "Loading Piper TTS model..."})

    # Download models if needed
    if not MODEL_PATH.exists():
        if not download_file(MODEL_URL, MODEL_PATH, "TTS model"):
            sys.exit(1)

    if not CONFIG_PATH.exists():
        if not download_file(CONFIG_URL, CONFIG_PATH, "TTS config"):
            sys.exit(1)

    try:
        voice = PiperVoice.load(str(MODEL_PATH), str(CONFIG_PATH))

        syn_config = SynthesisConfig(
            speaker_id=TTS_SPEAKER_ID,
            length_scale=TTS_LENGTH_SCALE,
            noise_scale=TTS_NOISE_SCALE,
            noise_w_scale=TTS_NOISE_W_SCALE,
            volume=1.0,
            normalize_audio=True,
        )

        send_response("status", {"message": "Piper TTS model loaded successfully"})
    except Exception as e:
        send_response("error", {"message": f"Failed to load Piper model: {e}"})
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

        elif cmd == "synthesize":
            # Sanitize input text to remove invalid Unicode
            text = sanitize_text(request.get("text", ""))
            streaming = request.get("streaming", True)  # Default to streaming

            if not text:
                send_response("error", {"message": "No text provided"})
                continue

            try:
                if streaming:
                    # Streaming mode: send each chunk as it's generated
                    chunk_index = 0
                    for audio_chunk in voice.synthesize(text, syn_config=syn_config):
                        wav_bytes = audio_to_wav_bytes(
                            audio_chunk.audio_int16_array,
                            audio_chunk.sample_rate
                        )
                        audio_b64 = base64.b64encode(wav_bytes).decode("utf-8")

                        send_response("audio_chunk", {
                            "audio": audio_b64,
                            "sample_rate": audio_chunk.sample_rate,
                            "chunk_index": chunk_index,
                        })
                        chunk_index += 1

                    # Signal end of stream
                    send_response("audio_done", {"total_chunks": chunk_index})
                else:
                    # Non-streaming mode: collect all chunks then send
                    audio_arrays = []
                    sample_rate = None

                    for audio_chunk in voice.synthesize(text, syn_config=syn_config):
                        audio_arrays.append(audio_chunk.audio_int16_array)
                        if sample_rate is None:
                            sample_rate = audio_chunk.sample_rate

                    if not audio_arrays:
                        send_response("error", {"message": "No audio generated"})
                        continue

                    audio_data = np.concatenate(audio_arrays)
                    wav_bytes = audio_to_wav_bytes(audio_data, sample_rate)
                    audio_b64 = base64.b64encode(wav_bytes).decode("utf-8")

                    send_response("audio", {
                        "audio": audio_b64,
                        "sample_rate": sample_rate,
                    })

            except Exception as e:
                send_response("error", {"message": f"Synthesis failed: {e}"})

        elif cmd == "ping":
            send_response("pong", {})

        else:
            send_response("error", {"message": f"Unknown command: {cmd}"})


if __name__ == "__main__":
    main()
