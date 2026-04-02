#!/usr/bin/env python3
"""
AIVA Engine - Combined inference server for AI Voice Assistant
Handles LLM, STT, and TTS in a single process
Communicates via stdin/stdout JSON protocol

Commands:
  - {"cmd": "init", "service": "llm|stt|tts"} - Initialize a service
  - {"cmd": "generate", "messages": [...]} - LLM generation
  - {"cmd": "transcribe", "audio": "base64..."} - STT transcription
  - {"cmd": "synthesize", "text": "...", "streaming": true} - TTS synthesis
  - {"cmd": "ping"} - Health check
  - {"cmd": "quit"} - Shutdown
"""

import sys
import json
import wave
import io
import base64
import os
import tempfile
import urllib.request
import shutil
from pathlib import Path

# Determine paths based on whether we're running as exe or script
if getattr(sys, 'frozen', False):
    # Running as PyInstaller exe
    SCRIPT_DIR = Path(sys.executable).parent.parent
else:
    # Running as Python script
    SCRIPT_DIR = Path(__file__).parent.parent

SETTINGS_PATH = SCRIPT_DIR / "settings.json"

# Default values (fallback if settings.json is missing or incomplete)
DEFAULTS = {
    "models": {
        "llm": {
            "path": "models/model.gguf",
            "download_url": "https://huggingface.co/unsloth/Qwen3-0.6B-GGUF/resolve/main/Qwen3-0.6B-Q4_K_M.gguf"
        },
        "stt": {
            "cache_dir": "models/hf_cache"
        },
        "tts": {
            "model_path": "models/tts/piper/model.onnx",
            "config_path": "models/tts/piper/model.onnx.json",
            "download_url": "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/libritts_r/medium/en_US-libritts_r-medium.onnx",
            "config_download_url": "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/libritts_r/medium/en_US-libritts_r-medium.onnx.json"
        }
    },
    "tts_settings": {
        "speaker_id": 237,
        "length_scale": 0.9,
        "noise_scale": 0.6,
        "noise_w_scale": 0.3
    },
    "llm_settings": {
        "ctx_length": 0
    }
}


def load_settings() -> dict:
    """Load settings from settings.json with fallback to defaults."""
    settings = DEFAULTS.copy()

    if SETTINGS_PATH.exists():
        try:
            with open(SETTINGS_PATH, 'r') as f:
                user_settings = json.load(f)
            # Deep merge user settings into defaults
            for key, value in user_settings.items():
                if key in settings and isinstance(value, dict):
                    settings[key] = {**settings[key], **value}
                else:
                    settings[key] = value
        except Exception as e:
            print(f"Warning: Failed to load settings.json: {e}", file=sys.stderr)

    return settings


# Load settings
SETTINGS = load_settings()

# Model paths (resolved from settings)
LLM_MODEL_PATH = SCRIPT_DIR / SETTINGS["models"]["llm"]["path"]
TTS_MODEL_PATH = SCRIPT_DIR / SETTINGS["models"]["tts"]["model_path"]
TTS_CONFIG_PATH = SCRIPT_DIR / SETTINGS["models"]["tts"]["config_path"]
STT_CACHE_DIR = SCRIPT_DIR / SETTINGS["models"]["stt"]["cache_dir"]

# Model download URLs
MODEL_URLS = {
    "llm": SETTINGS["models"]["llm"].get("download_url", ""),
    "tts_model": SETTINGS["models"]["tts"].get("download_url", ""),
    "tts_config": SETTINGS["models"]["tts"].get("config_download_url", ""),
}

# TTS Settings
TTS_SPEAKER_ID = SETTINGS["tts_settings"].get("speaker_id", 237)
TTS_LENGTH_SCALE = SETTINGS["tts_settings"].get("length_scale", 0.9)
TTS_NOISE_SCALE = SETTINGS["tts_settings"].get("noise_scale", 0.6)
TTS_NOISE_W_SCALE = SETTINGS["tts_settings"].get("noise_w_scale", 0.3)

# LLM Settings
LLM_CTX_LENGTH = SETTINGS.get("llm_settings", {}).get("ctx_length", 0)


def sanitize_text(text: str) -> str:
    """Remove invalid Unicode characters (surrogates, etc.) that can't be encoded."""
    if not isinstance(text, str):
        return text
    # Remove surrogate pairs and other problematic characters
    # Surrogates are in range U+D800 to U+DFFF
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
    # This ensures valid JSON output even if data contains unusual Unicode
    print(json.dumps(response, ensure_ascii=True), flush=True)


def download_file(url: str, dest_path: Path, desc: str = "file") -> bool:
    """Download a file with progress reporting."""
    if not url:
        return False

    try:
        send_response("status", {"message": f"Downloading {desc}..."})
        dest_path.parent.mkdir(parents=True, exist_ok=True)

        # Download to temp file first
        temp_path = dest_path.with_suffix('.tmp')

        with urllib.request.urlopen(url) as response:
            total_size = int(response.headers.get('Content-Length', 0))
            downloaded = 0
            chunk_size = 1024 * 1024  # 1MB chunks

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
                            "message": f"Downloading {desc}: {pct}% ({downloaded // (1024*1024)}MB)"
                        })

        # Move temp to final destination
        shutil.move(str(temp_path), str(dest_path))
        send_response("status", {"message": f"Downloaded {desc} successfully"})
        return True

    except Exception as e:
        send_response("error", {"message": f"Failed to download {desc}: {e}"})
        if temp_path.exists():
            temp_path.unlink()
        return False


class LLMService:
    """LLM inference using llama-cpp-python"""

    def __init__(self):
        self.llm = None
        self.loaded = False

    def load(self) -> bool:
        if self.loaded:
            return True

        send_response("status", {"message": "Loading LLM model..."})

        # Check if model exists, download if not
        if not LLM_MODEL_PATH.exists():
            if MODEL_URLS.get("llm"):
                if not download_file(MODEL_URLS["llm"], LLM_MODEL_PATH, "LLM model"):
                    return False
            else:
                send_response("error", {
                    "message": f"LLM model not found: {LLM_MODEL_PATH}\n"
                               "Please download manually or set MODEL_URLS['llm']"
                })
                return False

        try:
            from llama_cpp import Llama

            self.llm = Llama(
                model_path=str(LLM_MODEL_PATH),
                n_ctx=LLM_CTX_LENGTH,
                n_threads=4,
                verbose=False,
            )
            self.loaded = True
            send_response("status", {"message": "LLM model loaded successfully"})
            return True

        except Exception as e:
            send_response("error", {"message": f"Failed to load LLM: {e}"})
            return False

    def generate(self, messages: list, max_tokens: int = 256):
        if not self.loaded:
            send_response("error", {"message": "LLM not loaded"})
            return

        # Build prompt with chat template
        system_prompt = "You are a helpful AI assistant. Keep responses concise. /no_think"
        prompt = f"<|im_start|>system\n{system_prompt}<|im_end|>\n"

        for msg in messages:
            role = msg.get("role", "user")
            content = msg.get("content", "")
            prompt += f"<|im_start|>{role}\n{content}<|im_end|>\n"

        prompt += "<|im_start|>assistant\n"

        send_response("generation_start", {})

        try:
            full_response = ""
            in_think_block = False

            for output in self.llm(
                prompt,
                max_tokens=max_tokens,
                stop=["<|im_end|>", "<|endoftext|>"],
                stream=True,
            ):
                token = output["choices"][0]["text"]

                # Filter <think> blocks
                if "<think>" in token:
                    in_think_block = True
                    continue
                if "</think>" in token:
                    in_think_block = False
                    continue
                if in_think_block:
                    continue

                full_response += token
                send_response("token", {"token": token})

            send_response("generation_end", {"full_response": full_response.strip()})

        except Exception as e:
            send_response("error", {"message": f"Generation failed: {e}"})


class STTService:
    """Speech-to-text using faster-whisper"""

    def __init__(self):
        self.model = None
        self.loaded = False

    def load(self) -> bool:
        if self.loaded:
            return True

        send_response("status", {"message": "Loading STT model..."})

        # Set HuggingFace cache
        STT_CACHE_DIR.mkdir(parents=True, exist_ok=True)
        os.environ["HF_HOME"] = str(STT_CACHE_DIR)
        os.environ["HF_HUB_CACHE"] = str(STT_CACHE_DIR)
        os.environ["HF_HUB_DISABLE_SYMLINKS_WARNING"] = "1"

        try:
            from faster_whisper import WhisperModel

            # This will auto-download from HuggingFace if not cached
            self.model = WhisperModel(
                "base",
                device="cpu",
                compute_type="int8",
                download_root=str(STT_CACHE_DIR),
            )
            self.loaded = True
            send_response("status", {"message": "STT model loaded successfully"})
            return True

        except Exception as e:
            send_response("error", {"message": f"Failed to load STT: {e}"})
            return False

    def transcribe(self, audio_b64: str):
        if not self.loaded:
            send_response("error", {"message": "STT not loaded"})
            return

        try:
            # Decode audio
            audio_bytes = base64.b64decode(audio_b64)

            # Save to temp file
            with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
                f.write(audio_bytes)
                temp_path = f.name

            try:
                segments, info = self.model.transcribe(
                    temp_path,
                    beam_size=5,
                    language=None,
                    vad_filter=True,
                )

                text = " ".join(seg.text.strip() for seg in segments)

                send_response("transcription", {
                    "text": text,
                    "language": info.language,
                    "language_probability": info.language_probability,
                })

            finally:
                os.unlink(temp_path)

        except Exception as e:
            send_response("error", {"message": f"Transcription failed: {e}"})


class TTSService:
    """Text-to-speech using Piper"""

    def __init__(self):
        self.voice = None
        self.syn_config = None
        self.loaded = False

    def load(self) -> bool:
        if self.loaded:
            return True

        send_response("status", {"message": "Loading TTS model..."})

        # Check if model exists, download if not
        if not TTS_MODEL_PATH.exists():
            if MODEL_URLS.get("tts_model"):
                if not download_file(MODEL_URLS["tts_model"], TTS_MODEL_PATH, "TTS model"):
                    return False
            else:
                send_response("error", {
                    "message": f"TTS model not found: {TTS_MODEL_PATH}\n"
                               "Please download manually or set MODEL_URLS['tts_model']"
                })
                return False

        if not TTS_CONFIG_PATH.exists():
            if MODEL_URLS.get("tts_config"):
                if not download_file(MODEL_URLS["tts_config"], TTS_CONFIG_PATH, "TTS config"):
                    return False
            else:
                send_response("error", {
                    "message": f"TTS config not found: {TTS_CONFIG_PATH}\n"
                               "Please download manually or set MODEL_URLS['tts_config']"
                })
                return False

        try:
            from piper import PiperVoice
            from piper.voice import SynthesisConfig
            import numpy as np

            self.voice = PiperVoice.load(str(TTS_MODEL_PATH), str(TTS_CONFIG_PATH))
            self.syn_config = SynthesisConfig(
                speaker_id=TTS_SPEAKER_ID,
                length_scale=TTS_LENGTH_SCALE,
                noise_scale=TTS_NOISE_SCALE,
                noise_w_scale=TTS_NOISE_W_SCALE,
                volume=1.0,
                normalize_audio=True,
            )
            self.loaded = True
            send_response("status", {"message": "TTS model loaded successfully"})
            return True

        except Exception as e:
            send_response("error", {"message": f"Failed to load TTS: {e}"})
            return False

    def synthesize(self, text: str, streaming: bool = True):
        if not self.loaded:
            send_response("error", {"message": "TTS not loaded"})
            return

        import numpy as np

        try:
            if streaming:
                chunk_index = 0
                for audio_chunk in self.voice.synthesize(text, syn_config=self.syn_config):
                    wav_bytes = self._audio_to_wav(
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

                send_response("audio_done", {"total_chunks": chunk_index})
            else:
                # Non-streaming
                audio_arrays = []
                sample_rate = None

                for audio_chunk in self.voice.synthesize(text, syn_config=self.syn_config):
                    audio_arrays.append(audio_chunk.audio_int16_array)
                    if sample_rate is None:
                        sample_rate = audio_chunk.sample_rate

                if not audio_arrays:
                    send_response("error", {"message": "No audio generated"})
                    return

                audio_data = np.concatenate(audio_arrays)
                wav_bytes = self._audio_to_wav(audio_data, sample_rate)
                audio_b64 = base64.b64encode(wav_bytes).decode("utf-8")

                send_response("audio", {
                    "audio": audio_b64,
                    "sample_rate": sample_rate,
                })

        except Exception as e:
            send_response("error", {"message": f"Synthesis failed: {e}"})

    def _audio_to_wav(self, audio_data, sample_rate: int) -> bytes:
        buffer = io.BytesIO()
        with wave.open(buffer, "wb") as wav_file:
            wav_file.setnchannels(1)
            wav_file.setsampwidth(2)
            wav_file.setframerate(sample_rate)
            wav_file.writeframes(audio_data.tobytes())
        return buffer.getvalue()


def main():
    send_response("status", {"message": "AIVA Engine starting..."})

    # Initialize services (lazy loading)
    llm = LLMService()
    stt = STTService()
    tts = TTSService()

    send_response("status", {"message": "AIVA Engine ready"})

    # Main loop
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

        elif cmd == "init":
            service = request.get("service", "")
            if service == "llm":
                llm.load()
            elif service == "stt":
                stt.load()
            elif service == "tts":
                tts.load()
            elif service == "all":
                llm.load()
                stt.load()
                tts.load()
            else:
                send_response("error", {"message": f"Unknown service: {service}"})

        elif cmd == "generate":
            if not llm.loaded:
                llm.load()
            if llm.loaded:
                # Sanitize input messages to remove invalid Unicode
                messages = request.get("messages", [])
                messages = sanitize_data(messages)
                max_tokens = request.get("max_tokens", 0)
                llm.generate(messages, max_tokens)

        elif cmd == "transcribe":
            if not stt.loaded:
                stt.load()
            if stt.loaded:
                audio = request.get("audio", "")
                stt.transcribe(audio)

        elif cmd == "synthesize":
            if not tts.loaded:
                tts.load()
            if tts.loaded:
                # Sanitize input text to remove invalid Unicode
                text = sanitize_text(request.get("text", ""))
                streaming = request.get("streaming", True)
                tts.synthesize(text, streaming)

        elif cmd == "ping":
            send_response("pong", {
                "llm_loaded": llm.loaded,
                "stt_loaded": stt.loaded,
                "tts_loaded": tts.loaded,
            })

        else:
            send_response("error", {"message": f"Unknown command: {cmd}"})


if __name__ == "__main__":
    main()
