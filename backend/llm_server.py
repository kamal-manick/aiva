#!/usr/bin/env python3
"""
LLM Server for Rust Voice Assistant
Communicates via stdin/stdout JSON protocol
"""

import sys
import json
import re
import urllib.request
import shutil
from pathlib import Path

# Model path relative to this script
SCRIPT_DIR = Path(__file__).parent.parent
SETTINGS_PATH = SCRIPT_DIR / "settings.json"

# Default values (fallback if settings.json is missing or incomplete)
DEFAULT_MODEL_PATH = "models/model.gguf"
DEFAULT_MODEL_URL = "https://huggingface.co/unsloth/Qwen3-0.6B-GGUF/resolve/main/Qwen3-0.6B-Q4_K_M.gguf"


def load_settings() -> tuple[Path, str]:
    """Load LLM settings from settings.json with fallback to defaults."""
    model_path = SCRIPT_DIR / DEFAULT_MODEL_PATH
    model_url = DEFAULT_MODEL_URL
    ctx_length = 0

    if SETTINGS_PATH.exists():
        try:
            with open(SETTINGS_PATH, 'r') as f:
                settings = json.load(f)
        
            llm_models = settings.get("models", {}).get("llm", {})
            if "path" in llm_models:
                model_path = SCRIPT_DIR / llm_models["path"]
            if "download_url" in llm_models:
                model_url = llm_models["download_url"]
            
            llm_settings = settings.get("llm_settings", {})
            if "ctx_length" in llm_settings:
                ctx_length = llm_settings["ctx_length"]
        
        except Exception as e:
            print(f"Warning: Failed to load settings.json: {e}", file=sys.stderr)

    return model_path, model_url, ctx_length

MODEL_PATH, MODEL_URL, CTX_TOKEN_LENGTH = load_settings()

# Qwen3 chat template
SYSTEM_PROMPT = "You are a helpful voice assistant. Keep responses concise and conversational. /no_think"


def strip_thinking(text: str) -> str:
    """Remove <think>...</think> blocks from response."""
    return re.sub(r"<think>.*?</think>\s*", "", text, flags=re.DOTALL).strip()


def create_prompt(messages: list[dict]) -> str:
    """Format messages for Qwen3 chat template."""
    prompt_parts = []

    # Add system message
    prompt_parts.append(f"<|im_start|>system\n{SYSTEM_PROMPT}<|im_end|>")

    # Add conversation history
    for msg in messages:
        role = msg.get("role", "user")
        content = msg.get("content", "")
        prompt_parts.append(f"<|im_start|>{role}\n{content}<|im_end|>")

    # Add assistant prefix for generation
    prompt_parts.append("<|im_start|>assistant\n")

    return "\n".join(prompt_parts)


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


def download_model():
    """Download LLM model if not present."""
    if MODEL_PATH.exists():
        return True

    send_response("status", {"message": f"Downloading LLM model ({MODEL_URL})..."})

    try:
        MODEL_PATH.parent.mkdir(parents=True, exist_ok=True)
        temp_path = MODEL_PATH.with_suffix('.tmp')

        with urllib.request.urlopen(MODEL_URL) as response:
            total_size = int(response.headers.get('Content-Length', 0))
            downloaded = 0
            chunk_size = 1024 * 1024  # 1MB

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
                            "message": f"Downloading LLM: {pct}% ({downloaded // (1024*1024)}MB)"
                        })

        shutil.move(str(temp_path), str(MODEL_PATH))
        send_response("status", {"message": "LLM model downloaded successfully"})
        return True

    except Exception as e:
        send_response("error", {"message": f"Failed to download LLM model: {e}"})
        if temp_path.exists():
            temp_path.unlink()
        return False


def main():
    from llama_cpp import Llama

    # Send startup status
    send_response("status", {"message": "Loading model..."})

    # Download model if needed
    if not MODEL_PATH.exists():
        if not download_model():
            sys.exit(1)

    try:
        llm = Llama(
            model_path=str(MODEL_PATH),
            n_ctx=CTX_TOKEN_LENGTH, # Context window
            n_threads=4,            # CPU threads
            verbose=False,          # Suppress llama.cpp output
        )
        send_response("status", {"message": "Model loaded successfully"})
    except Exception as e:
        send_response("error", {"message": f"Failed to load model: {e}"})
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

        elif cmd == "generate":
            # Sanitize input messages to remove invalid Unicode
            messages = sanitize_data(request.get("messages", []))
            prompt = create_prompt(messages)

            try:
                # Stream tokens
                send_response("generation_start", {})

                full_response = ""
                in_thinking = False

                for output in llm(
                    prompt,
                    max_tokens=256,
                    stop=["<|im_end|>", "<|im_start|>"],
                    stream=True,
                    temperature=0.7,
                    top_p=0.9,
                ):
                    token = output["choices"][0]["text"]
                    full_response += token

                    # Filter out <think> blocks during streaming
                    if "<think>" in token:
                        in_thinking = True
                    elif "</think>" in token:
                        in_thinking = False
                        continue

                    if not in_thinking and token.strip():
                        send_response("token", {"token": token})

                # Strip thinking blocks from final response
                clean_response = strip_thinking(full_response)
                send_response("generation_end", {"full_response": clean_response})

            except Exception as e:
                send_response("error", {"message": f"Generation failed: {e}"})

        elif cmd == "ping":
            send_response("pong", {})

        else:
            send_response("error", {"message": f"Unknown command: {cmd}"})


if __name__ == "__main__":
    main()
