# AIVA - Packaging Script
# Creates a distributable package with AIVA.exe (Rust) + AIVAEngine.exe (Python) + models

param(
    [switch]$RebuildRust,      # Rebuild Rust frontend even if exe exists
    [switch]$RebuildPython,    # Rebuild Python backend even if exe exists
    [switch]$SkipModels,
    [string]$OutputName = "AIVA"
)

$ErrorActionPreference = "Stop"

# Configuration
$rootFolder = Split-Path -Parent $PSScriptRoot
$packageFolder = $PSScriptRoot
$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$distFolder = Join-Path $packageFolder "dist"
$outputFolder = Join-Path $distFolder $OutputName

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "AIVA - Packaging Script" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Root folder: $rootFolder" -ForegroundColor Yellow
Write-Host "Output folder: $outputFolder" -ForegroundColor Yellow
Write-Host ""
Write-Host "Options:" -ForegroundColor Yellow
Write-Host "  RebuildRust: $RebuildRust" -ForegroundColor Gray
Write-Host "  RebuildPython: $RebuildPython" -ForegroundColor Gray
Write-Host "  SkipModels: $SkipModels" -ForegroundColor Gray
Write-Host ""

# Clean previous build
if (Test-Path $outputFolder) {
    Write-Host "Cleaning previous build..." -ForegroundColor Yellow
    Remove-Item -Path $outputFolder -Recurse -Force
}
New-Item -ItemType Directory -Path $outputFolder -Force | Out-Null

# Step 1: Build Rust application in release mode
Write-Host ""
Write-Host "Step 1: Rust frontend (AIVA.exe)..." -ForegroundColor Green

$rustExe = Join-Path $rootFolder "target\release\rust_exercise9.exe"
$needRustBuild = $RebuildRust -or (-not (Test-Path $rustExe))

if ($needRustBuild) {
    Write-Host "  Building Rust application (release mode)..." -ForegroundColor Yellow
    Push-Location $rootFolder
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) {
            throw "Cargo build failed"
        }
    } finally {
        Pop-Location
    }
} else {
    Write-Host "  Using existing Rust executable (use -RebuildRust to force rebuild)" -ForegroundColor Gray
}

# Step 2: Copy Rust executable as AIVA.exe
Write-Host ""
Write-Host "Step 2: Copying Rust executable..." -ForegroundColor Green
if (-not (Test-Path $rustExe)) {
    throw "Rust executable not found at: $rustExe"
}
Copy-Item -Path $rustExe -Destination (Join-Path $outputFolder "AIVA.exe")
$size = [math]::Round((Get-Item $rustExe).Length / 1MB, 1)
Write-Host "  + AIVA.exe ($size MB)" -ForegroundColor Gray

# Step 3: Build/Copy Python backend (AIVAEngine.exe)
Write-Host ""
Write-Host "Step 3: Python backend (AIVAEngine.exe)..." -ForegroundColor Green

$backendFolder = Join-Path $outputFolder "backend"
New-Item -ItemType Directory -Path $backendFolder -Force | Out-Null

$sourceBackend = Join-Path $rootFolder "backend"
$backendDist = Join-Path $sourceBackend "dist"
$combinedEngine = Join-Path $backendDist "AIVAEngine.exe"

$needPythonBuild = $RebuildPython -or (-not (Test-Path $combinedEngine))

if ($needPythonBuild) {
    Write-Host "  Building AIVAEngine.exe (this may take a while)..." -ForegroundColor Yellow

    # Find Python in venv
    $pythonExe = Join-Path $sourceBackend "Scripts\python.exe"
    if (-not (Test-Path $pythonExe)) {
        throw "Python venv not found at: $pythonExe"
    }

    $buildScript = Join-Path $sourceBackend "build_exe.py"
    if (-not (Test-Path $buildScript)) {
        throw "Build script not found at: $buildScript"
    }

    Push-Location $sourceBackend
    try {
        & $pythonExe $buildScript --combined
        if ($LASTEXITCODE -ne 0) {
            throw "Python build failed"
        }
    } finally {
        Pop-Location
    }
} else {
    Write-Host "  Using existing AIVAEngine.exe (use -RebuildPython to force rebuild)" -ForegroundColor Gray
}

# Copy AIVAEngine.exe
if (-not (Test-Path $combinedEngine)) {
    throw "AIVAEngine.exe not found at: $combinedEngine"
}
Copy-Item -Path $combinedEngine -Destination $backendFolder
$size = [math]::Round((Get-Item $combinedEngine).Length / 1MB, 1)
Write-Host "  + backend\AIVAEngine.exe ($size MB)" -ForegroundColor Gray

# Step 4: Copy settings.json
Write-Host ""
Write-Host "Step 4: Copying settings.json..." -ForegroundColor Green
$settingsFile = Join-Path $rootFolder "settings.json"
if (Test-Path $settingsFile) {
    Copy-Item -Path $settingsFile -Destination $outputFolder
    Write-Host "  + settings.json" -ForegroundColor Gray
} else {
    Write-Host "  Warning: settings.json not found, using defaults" -ForegroundColor Yellow
}

# Step 5: Copy models
if (-not $SkipModels) {
    Write-Host ""
    Write-Host "Step 5: Copying models..." -ForegroundColor Green
    $modelsSource = Join-Path $rootFolder "models"
    $modelsDest = Join-Path $outputFolder "models"

    if (Test-Path $modelsSource) {
        New-Item -ItemType Directory -Path $modelsDest -Force | Out-Null

        # Copy LLM model (rename to generic model.gguf)
        $llmModel = Join-Path $modelsSource "Qwen3-0.6B-Q4_K_M.gguf"
        if (Test-Path $llmModel) {
            Copy-Item -Path $llmModel -Destination (Join-Path $modelsDest "model.gguf")
            $size = [math]::Round((Get-Item $llmModel).Length / 1MB, 1)
            Write-Host "  + models\model.gguf ($size MB)" -ForegroundColor Gray
        }

        # Copy TTS models (rename to generic model.onnx)
        $ttsSource = Join-Path $modelsSource "tts\piper"
        $ttsDest = Join-Path $modelsDest "tts\piper"
        if (Test-Path $ttsSource) {
            New-Item -ItemType Directory -Path $ttsDest -Force | Out-Null

            # Copy and rename onnx model
            $onnxFiles = Get-ChildItem -Path $ttsSource -Filter "*.onnx" -File | Where-Object { $_.Name -notmatch "\.json$" }
            if ($onnxFiles) {
                $onnxFile = $onnxFiles | Select-Object -First 1
                Copy-Item -Path $onnxFile.FullName -Destination (Join-Path $ttsDest "model.onnx")
                Write-Host "  + models\tts\piper\model.onnx" -ForegroundColor Gray
            }

            # Copy and rename onnx config
            $configFiles = Get-ChildItem -Path $ttsSource -Filter "*.onnx.json" -File
            if ($configFiles) {
                $configFile = $configFiles | Select-Object -First 1
                Copy-Item -Path $configFile.FullName -Destination (Join-Path $ttsDest "model.onnx.json")
                Write-Host "  + models\tts\piper\model.onnx.json" -ForegroundColor Gray
            }
        }

        # Copy STT cache (faster-whisper will download if not present)
        $sttCache = Join-Path $modelsSource "hf_cache"
        if (Test-Path $sttCache) {
            $sttDest = Join-Path $modelsDest "hf_cache"
            Copy-Item -Path $sttCache -Destination $sttDest -Recurse
            Write-Host "  + models\hf_cache\* (STT cache)" -ForegroundColor Gray
        }
    } else {
        Write-Host "  Warning: Models folder not found, skipping..." -ForegroundColor Yellow
    }
} else {
    Write-Host ""
    Write-Host "Step 5: Skipping models (user must provide)..." -ForegroundColor Yellow
}

# Step 6: Create launcher script (hidden console)
Write-Host ""
Write-Host "Step 6: Creating launcher scripts..." -ForegroundColor Green

# VBScript launcher to hide console window
$vbsContent = @'
Set WshShell = CreateObject("WScript.Shell")
WshShell.Run Chr(34) & CreateObject("Scripting.FileSystemObject").GetParentFolderName(WScript.ScriptFullName) & "\AIVA.exe" & Chr(34), 0, False
Set WshShell = Nothing
'@
$vbsContent | Out-File -FilePath (Join-Path $outputFolder "Launch.vbs") -Encoding ASCII
Write-Host "  + Launch.vbs (hidden console)" -ForegroundColor Gray

# Batch launcher for debugging
$batContent = @'
@echo off
REM AIVA Launcher (with console for debugging)
cd /d "%~dp0"
AIVA.exe
pause
'@
$batContent | Out-File -FilePath (Join-Path $outputFolder "Launch_Debug.bat") -Encoding ASCII
Write-Host "  + Launch_Debug.bat (shows console)" -ForegroundColor Gray

# Step 7: Create README
Write-Host ""
Write-Host "Step 7: Creating README..." -ForegroundColor Green

$readmeContent = @"
# AIVA - AI Voice Assistant

A fully local AI voice assistant powered by:
- LLM (GGUF format) - Language model for conversations
- Faster-Whisper (STT) - Speech-to-text transcription
- Piper TTS - Text-to-speech synthesis

All processing happens locally on your machine - no internet required after setup.

## Quick Start

1. Double-click `Launch.vbs` to start (no console window)
   - Or use `Launch_Debug.bat` to see console output for troubleshooting
2. Wait for all models to load (LLM, STT, TTS status shows "ready")
3. Type a message or click the Mic button to talk

## Files

- `AIVA.exe` - Main application (Rust GUI)
- `Launch.vbs` - Launcher (hides console window)
- `Launch_Debug.bat` - Launcher with console (for debugging)
- `backend/AIVAEngine.exe` - AI engine (LLM + STT + TTS)
- `settings.json` - Configuration file for model paths and settings
- `models/` - AI models
  - `model.gguf` - LLM model
  - `tts/piper/model.onnx` - TTS voice model
  - `tts/piper/model.onnx.json` - TTS voice config
  - `hf_cache/` - STT model cache

## Configuration

Edit `settings.json` to customize:
- Model file paths
- Download URLs for auto-download
- TTS voice settings (speaker_id, length_scale, noise_scale, noise_w_scale)

## System Requirements

- Windows 10/11 (64-bit)
- 4GB+ RAM recommended
- Microphone for voice input
- Speakers/headphones for TTS output

## Troubleshooting

If the app fails to start:
1. Make sure all files are extracted (not running from ZIP)
2. Check that the `models` folder contains the required models
3. Run `Launch_Debug.bat` to see error messages
4. Ensure no antivirus is blocking the executables

## Privacy

All AI processing runs locally on your computer.
No data is sent to external servers.
"@
$readmeContent | Out-File -FilePath (Join-Path $outputFolder "README.txt") -Encoding UTF8
Write-Host "  + README.txt" -ForegroundColor Gray

# Step 8: Calculate total size
Write-Host ""
Write-Host "Step 8: Calculating package size..." -ForegroundColor Green
$totalSize = (Get-ChildItem -Path $outputFolder -Recurse | Measure-Object -Property Length -Sum).Sum
$totalSizeMB = [math]::Round($totalSize / 1MB, 1)
$totalSizeGB = [math]::Round($totalSize / 1GB, 2)

# Step 9: Create ZIP archive
Write-Host ""
Write-Host "Step 9: Creating ZIP archive..." -ForegroundColor Green
$zipPath = Join-Path $distFolder "$OutputName`_$timestamp.zip"
Compress-Archive -Path "$outputFolder\*" -DestinationPath $zipPath -Force
$zipSize = [math]::Round((Get-Item $zipPath).Length / 1MB, 1)
Write-Host "  + $OutputName`_$timestamp.zip ($zipSize MB)" -ForegroundColor Gray

# Summary
Write-Host ""
Write-Host "========================================" -ForegroundColor Green
Write-Host "Packaging completed successfully!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green
Write-Host ""
Write-Host "Output folder: $outputFolder" -ForegroundColor White
Write-Host "Total size: $totalSizeMB MB ($totalSizeGB GB)" -ForegroundColor White
Write-Host ""
Write-Host "ZIP archive: $zipPath" -ForegroundColor White
Write-Host "ZIP size: $zipSize MB" -ForegroundColor White
Write-Host ""
Write-Host "To distribute:" -ForegroundColor Yellow
Write-Host "  1. Share the ZIP file, or" -ForegroundColor Gray
Write-Host "  2. Copy the '$OutputName' folder" -ForegroundColor Gray
Write-Host ""
