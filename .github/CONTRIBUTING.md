# Contributing

Thank you for your interest in AIVA. This repository is a sanitized reference implementation of a production system, published as a portfolio piece. Contributions that improve documentation, fix bugs, or extend the reference architecture are welcome.

## Getting Started

### Prerequisites

- **Rust** 1.75+ with Cargo
- **Python** 3.10+ with pip
- Windows 10/11 (primary target; Linux/macOS may work with minor adjustments)
- A microphone and speakers/headphones

### Setup

1. Clone the repository
2. Copy `settings.example.json` to `settings.json`
3. Create a Python virtual environment in `backend/`:
   ```
   cd backend
   python -m venv .
   Scripts/pip install -r requirements.txt
   ```
4. Build and run:
   ```
   cargo run
   ```
   Models will download automatically on first run.

## Development Guidelines

- **Rust code** follows standard Rust formatting (`cargo fmt`) and linting (`cargo clippy`).
- **Python code** follows PEP 8. The backend scripts are intentionally simple -- single-file services with no framework dependencies beyond the ML libraries.
- **IPC protocol changes** must be coordinated between the Rust client and Python server. Document any new message types.

## Reporting Issues

Use the GitHub issue templates for bug reports and feature requests. Include:
- Your OS version and hardware (CPU, RAM)
- Model files you're using
- Steps to reproduce
- Any error output from the console

## Code of Conduct

Be respectful. This is a personal portfolio project and contributions should be constructive.
