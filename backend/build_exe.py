#!/usr/bin/env python3
"""
Build script to create standalone executables for the Python servers.
Uses PyInstaller to bundle Python + dependencies into single exe files.

Usage:
    python build_exe.py [--combined | --separate | --all]

    --combined  Build single AIVAEngine.exe (recommended, smaller total size)
    --separate  Build separate llm_server.exe, stt_server.exe, tts_server.exe
    --all       Build both combined and separate executables

Requirements:
    pip install pyinstaller
"""

import subprocess
import sys
import shutil
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent
ROOT_DIR = SCRIPT_DIR.parent
DIST_DIR = SCRIPT_DIR / "dist"


def run_pyinstaller(script_name: str, output_name: str = None, hidden_imports: list = None, extra_args: list = None):
    """Run PyInstaller to build a single exe."""
    script_path = SCRIPT_DIR / script_name

    if not script_path.exists():
        print(f"Error: {script_path} not found")
        return False

    exe_name = output_name or script_path.stem

    cmd = [
        sys.executable, "-m", "PyInstaller",
        "--onefile",           # Single exe file
        "--console",           # Console app (needed for stdin/stdout)
        "--clean",             # Clean build
        "--noconfirm",         # Overwrite without asking
        f"--name={exe_name}",
        f"--distpath={DIST_DIR}",
        f"--workpath={SCRIPT_DIR / 'build'}",
        f"--specpath={SCRIPT_DIR / 'specs'}",
    ]

    # Add hidden imports
    if hidden_imports:
        for imp in hidden_imports:
            cmd.extend(["--hidden-import", imp])

    # Add extra args
    if extra_args:
        cmd.extend(extra_args)

    cmd.append(str(script_path))

    print(f"\n{'='*60}")
    print(f"Building {exe_name}.exe from {script_name}...")
    print(f"{'='*60}")
    print(f"Command: {' '.join(cmd)}")
    print()

    result = subprocess.run(cmd, cwd=SCRIPT_DIR)

    if result.returncode == 0:
        exe_path = DIST_DIR / f"{exe_name}.exe"
        if exe_path.exists():
            size_mb = exe_path.stat().st_size / (1024 * 1024)
            print(f"\nSuccess: {exe_name}.exe ({size_mb:.1f} MB)")
            return True

    print(f"\nFailed to build {exe_name}.exe")
    return False


def build_combined_engine():
    """Build combined AIVAEngine.exe with all services"""
    return run_pyinstaller(
        "aiva_engine.py",
        output_name="AIVAEngine",
        hidden_imports=[
            # LLM
            "llama_cpp",
            "llama_cpp.llama",
            "llama_cpp.llama_cpp",
            # STT
            "faster_whisper",
            "ctranslate2",
            "tokenizers",
            "huggingface_hub",
            # TTS
            "piper",
            "piper.voice",
            "onnxruntime",
            "numpy",
        ],
        extra_args=[
            "--collect-binaries=llama_cpp",
            "--collect-binaries=ctranslate2",
            "--collect-binaries=onnxruntime",
            "--collect-data=faster_whisper",
            "--collect-data=piper",
        ]
    )


def build_llm_server():
    """Build llm_server.exe"""
    return run_pyinstaller(
        "llm_server.py",
        hidden_imports=[
            "llama_cpp",
            "llama_cpp.llama",
            "llama_cpp.llama_cpp",
        ],
        extra_args=[
            "--collect-binaries=llama_cpp",
        ]
    )


def build_stt_server():
    """Build stt_server.exe"""
    return run_pyinstaller(
        "stt_server.py",
        hidden_imports=[
            "faster_whisper",
            "ctranslate2",
            "tokenizers",
            "huggingface_hub",
        ],
        extra_args=[
            "--collect-binaries=ctranslate2",
            "--collect-data=faster_whisper",
        ]
    )


def build_tts_server():
    """Build tts_server.exe"""
    return run_pyinstaller(
        "tts_server.py",
        hidden_imports=[
            "piper",
            "piper.voice",
            "onnxruntime",
            "numpy",
        ],
        extra_args=[
            "--collect-binaries=onnxruntime",
            "--collect-data=piper",
        ]
    )


def main():
    # Create output directories
    DIST_DIR.mkdir(exist_ok=True)
    (SCRIPT_DIR / "build").mkdir(exist_ok=True)
    (SCRIPT_DIR / "specs").mkdir(exist_ok=True)

    # Parse arguments
    args = sys.argv[1:] if len(sys.argv) > 1 else ["--combined"]

    results = {}

    if "--combined" in args or "--all" in args:
        print("\n" + "="*60)
        print("Building COMBINED engine (recommended)")
        print("="*60)
        results["AIVAEngine"] = build_combined_engine()

    if "--separate" in args or "--all" in args:
        print("\n" + "="*60)
        print("Building SEPARATE servers")
        print("="*60)
        results["llm_server"] = build_llm_server()
        results["stt_server"] = build_stt_server()
        results["tts_server"] = build_tts_server()

    # Summary
    print(f"\n{'='*60}")
    print("Build Summary")
    print(f"{'='*60}")

    for name, success in results.items():
        status = "OK" if success else "FAILED"
        exe_path = DIST_DIR / f"{name}.exe"
        if exe_path.exists():
            size_mb = exe_path.stat().st_size / (1024 * 1024)
            print(f"  {name}.exe: {status} ({size_mb:.1f} MB)")
        else:
            print(f"  {name}.exe: {status}")

    # Calculate total size
    total_size = 0
    for exe in DIST_DIR.glob("*.exe"):
        total_size += exe.stat().st_size

    print(f"\nTotal size: {total_size / (1024*1024):.1f} MB")
    print(f"Output: {DIST_DIR}")

    return 0 if all(results.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
