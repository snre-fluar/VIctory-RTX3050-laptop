@echo off
setlocal EnableExtensions DisableDelayedExpansion
if not defined VICTORY_GAME_DIR set "VICTORY_GAME_DIR=C:\Users\raulf\Desktop\pro\GTA6.Xbox.v1.0.2.1-CYBERLEEK"
if not exist "%VICTORY_GAME_DIR%\game_scarlett_master.exe" (
  echo Set VICTORY_GAME_DIR to your own dump folder containing game_scarlett_master.exe. 1>&2
  exit /b 2
)
set "runner=%~dp0target\release\victory.exe"
if not exist "%runner%" (
  echo Run cargo build --release -p victory first. 1>&2
  exit /b 2
)
if not exist "%~dp0nvidia-sdk\Streamline\bin\x64\sl.interposer.dll" (
  powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\get-nvidia-sdks.ps1"
)
if not exist "%~dp0amd-sdk\FidelityFX\bin\amd_fidelityfx_framegeneration_dx12.dll" (
  powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\get-amd-fsr.ps1"
)
set "VICTORY_CACHE_DIR=%LOCALAPPDATA%\VIctory\cache-pareto"
set "VICTORY_DISPLAY_MODE=1920x1080"
set "VICTORY_DLSS=1"
if not defined VICTORY_DLSS_MODE set "VICTORY_DLSS_MODE=performance"
set "VICTORY_FSR_FRAMEGEN=1"
if not defined VICTORY_FRAMEGEN_X set "VICTORY_FRAMEGEN_X=2"
set "VICTORY_FRAME_INTERVAL_MS=16"
set "VICTORY_STREAMLINE_DIR=%~dp0nvidia-sdk\Streamline\bin\x64"
set "VICTORY_FIDELITYFX_DIR=%~dp0amd-sdk\FidelityFX\bin"
set "VICTORY_TEXTURE_NO_WAIT=0"
set "VICTORY_RELAY=2"
set "VICTORY_WIDE_BACK=1"
set "VICTORY_CARD_REHASH_RUNS=1"
set "VICTORY_JOIN_COPIES=1"
set "VICTORY_INDIRECT_WINDOW=1"
set "VICTORY_RELAY_COPY=1"
set "VICTORY_CARD_STEPS=2"
set "VICTORY_INDIRECT_BATCH=2"
set "VICTORY_MAP_THREADS=1"
set "VICTORY_TITLE_PRIORITIES=0"
set "VICTORY_CARD_AWAKE=1"
set "VICTORY_SHOW_AFTER=1"
set "VICTORY_PORT_CONSTANTS=1"
set "VICTORY_PORT_DESCRIPTORS=0"
set "VICTORY_PORT_CBV=0"
set "VICTORY_PORT_GUARD=1"
set "VICTORY_PORT_JOURNAL=1"
set "VICTORY_AMBIENT=1.2"
set "VICTORY_AMBIENT_FLOOR=1.2"
set "VICTORY_SHADOW_LIFT=1.2"
set "VICTORY_AMBIENT_AIR=1.5"
set "VICTORY_AMBIENT_MOST=2"
set "VICTORY_THIN=1"
set "VICTORY_DYNAMIC_SLOTS=524288"
set "VICTORY_PIPELINE_WAIT_MS=500"
set "VICTORY_WINDOWS_BAR_MB=0"
if exist "%~dp0scripts\rt_off_hashes.txt" (
  for /f "usebackq delims=" %%H in ("%~dp0scripts\rt_off_hashes.txt") do set "VICTORY_SKIP_PIPELINES=%%H"
)
set "VICTORY_JOIN_PARTS=8"
set "VICTORY_UPLOAD_AHEAD=1"
set "VICTORY_LEVELS_ON_DEMAND=0"
set "VICTORY_TEXTURE_MAX_DIM=1024"
set "VICTORY_CARD_BUFFERS_MB=384"
set "VICTORY_TEXTURE_ROOM_PERCENT=93"
set "VICTORY_TEXTURE_STAGE_MB=256"
"%runner%" run --execute --console series-s "%VICTORY_GAME_DIR%\game_scarlett_master.exe"
