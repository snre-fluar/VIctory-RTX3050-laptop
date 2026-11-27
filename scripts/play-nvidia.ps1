# Starts the title on an NVIDIA laptop/desktop GPU using the cool_fox engine baseline
# (Series S 2-plane Compose UI, Wave32 fixes, synchronous bake uploads, residency guard),
# plus optional render-scale/upscaler hooks and automatic 4-6 GB VRAM / 16 GB RAM tuning.
#   scripts\play-nvidia.ps1 -Exe <path of your own copy's executable>
# -RenderScale     internal render scale (default '' = native Series S dynamic temporal upscaler; e.g. '0.5' for extra FPS)
# -UseFsr          use AMD FSR4 instead of the title's own temporal upscaler on Series S (heavier & falls back to NewBE on NVIDIA)
# -OwnUpscaler     use title's own temporal upscaler instead of FSR (enabled automatically on Series S NVIDIA unless -UseFsr)
# -SmoothMotion    run victory_sm.exe copy instead of victory.exe (RTX 40/50 only)
# -SeriesX         use Series X profile instead of Series S (heavier on 4 GB VRAM)
# -Safe            without thin lists / join parts / upload ahead (troubleshooting)
# -RayTracing      enable the title's ray-traced indirect light (heavier on 4 GB VRAM)
# -Log             write logs to a file instead of only the console
# -LogLevel        tracing level (default 'warn' for max FPS; use 'info' when capturing diag logs)
# -Build           release (default if built, fastest) or diag (full diagnostics)
# -More            extra overrides: -More @{VICTORY_RENDER_SCALE='0.5'}
param(
    [Parameter(Mandatory)][string]$Exe,
    [string]$RenderScale = '',
    [switch]$OwnUpscaler,
    [switch]$UseFsr,
    [switch]$SmoothMotion,
    [switch]$SeriesX,
    [switch]$Safe,
    [switch]$RayTracing,
    [string]$Log = '',
    [string]$LogLevel = 'warn',
    [ValidateSet('auto', 'diag', 'release')][string]$Build = 'auto',
    [hashtable]$More = @{}
)

# Detect discrete NVIDIA VRAM (MiB) and host RAM (GiB) to tune laptop headroom automatically.
$vramMb = 0
try {
    $smi = & nvidia-smi --query-gpu=memory.total --format=csv,noheader,nounits 2>$null
    if ($smi) {
        $vramMb = ($smi | ForEach-Object { ($_ -as [int]) } | Measure-Object -Maximum).Maximum
    }
} catch {}
if ($vramMb -le 0) {
    try {
        $vramMb = [int]((Get-CimInstance Win32_VideoController | Where-Object { $_.Name -match 'NVIDIA' } | Select-Object -First 1).AdapterRAM / 1MB)
    } catch {}
}
$ramGb = 0
try {
    $ramGb = [int][Math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB)
} catch {}

$lowVram = ($vramMb -gt 0 -and $vramMb -le 6144)
$lowRam = ($ramGb -gt 0 -and $ramGb -le 20)

$settings = @{
    # Fast cool_fox pareto-residency engine set (Series S / NVIDIA verified baseline).
    VICTORY_AMBIENT = '1.2'; VICTORY_AMBIENT_FLOOR = '1.2'; VICTORY_SHADOW_LIFT = '1.2'
    VICTORY_AMBIENT_AIR = '1.5'; VICTORY_AMBIENT_MOST = '2'
    VICTORY_TEXTURE_NO_WAIT = '0'; VICTORY_RELAY = '2'; VICTORY_WIDE_BACK = '1'; VICTORY_CARD_REHASH_RUNS = '1'
    VICTORY_JOIN_COPIES = '1'; VICTORY_INDIRECT_WINDOW = '1'; VICTORY_RELAY_COPY = '1'; VICTORY_CARD_STEPS = '2'
    VICTORY_INDIRECT_BATCH = '2'; VICTORY_MAP_THREADS = '1'; VICTORY_TITLE_PRIORITIES = '0'
    VICTORY_CARD_AWAKE = '1'; VICTORY_SHOW_AFTER = '1'
    # Keep safe speed ports on, but keep Port 2/5 (DESCRIPTORS/CBV) off to avoid black-face fallback misses.
    VICTORY_PORT_CONSTANTS = '1'; VICTORY_PORT_DESCRIPTORS = '0'
    VICTORY_PORT_CBV = '0'; VICTORY_PORT_GUARD = '1'; VICTORY_PORT_JOURNAL = '1'
    VICTORY_LEVELS_ON_DEMAND = '1'; VICTORY_CARD_BUFFERS_MIN_MB = '1'
    # Scrub NaN/Inf in pixel shader colour exports and float image stores (fixes black blobs/white blowouts).
    VICTORY_NAN_SCRUB = '1'
    # Brief wait once per new pipeline without multi-second frame stalls.
    VICTORY_PIPELINE_WAIT_MS = '500'
    # NVIDIA caps the shader-visible view heap at 1,000,000; 524,288 dynamic slots prevents dropped draws.
    VICTORY_DYNAMIC_SLOTS = '524288'
    # Disable D3D background shader recompile threads competing with 6P+4E laptop cores.
    VICTORY_D3D12_BACKGROUND = '0'
    VICTORY_LOG = $LogLevel
}

if (-not $SeriesX -and -not $UseFsr) {
    # Series S enables AMD FSR4 ACO by default, which fails ACO deserialization on PC (NewBE
    # fallback) and stalls/TDRs 4 GB laptop GPUs; use the title's lightweight temporal upscaler.
    $settings.VICTORY_OWN_UPSCALER = '1'
}
if ($RenderScale) {
    $settings.VICTORY_RENDER_SCALE = $RenderScale
    if (-not $SeriesX -and -not $UseFsr) {
        $settings.VICTORY_OWN_UPSCALER = '1'
    }
}
if ($OwnUpscaler) {
    $settings.VICTORY_OWN_UPSCALER = '1'
}

if ($Safe) {
    $settings.VICTORY_UNDER_WIDE = '0'; $settings.VICTORY_LAND_ECHO = '0'
} else {
    $settings.VICTORY_THIN = '1'
    $settings.VICTORY_JOIN_PARTS = '8'
    $settings.VICTORY_UPLOAD_AHEAD = '1'
}

if ($RayTracing) {
    $settings.VICTORY_BVH_BUILDER = '1'
} else {
    $settings.VICTORY_BVH_MISS = '1'
    $rtOffPath = Join-Path $PSScriptRoot 'rt_off_hashes.txt'
    if (Test-Path $rtOffPath) {
        $rtOffHashes = Get-Content $rtOffPath | ForEach-Object { $_.Trim() } | Where-Object { $_ -and -not $_.StartsWith('#') }
        if ($rtOffHashes.Count -gt 0) {
            $settings.VICTORY_SKIP_PIPELINES = ($rtOffHashes -join ',')
        }
    }
}

if ($lowRam -and -not $lowVram) {
    $settings.VICTORY_WINDOWS_BAR_MB = '3072'
}

if ($lowVram) {
    # 4-6 GB VRAM laptops (e.g. RTX 3050 4 GB): 1080p output with 0.5x own upscaler,
    # cap streamed textures at 512px (LEVELS_ON_DEMAND=0), keep BAR windows off (0) so local
    # VRAM isn't spilled to shared RAM, keep 384 MB card buffers so resource::room() stays open
    # for textures, and stage up to 256 MB / 500 ms per frame so textures stream without stalling.
    $settings.VICTORY_DISPLAY_MODE = '1920x1080'
    $settings.VICTORY_OWN_UPSCALER = '1'
    $settings.VICTORY_RENDER_SCALE = '0.5'
    $settings.VICTORY_LEVELS_ON_DEMAND = '0'
    $settings.VICTORY_TEXTURE_MAX_DIM = '512'
    $settings.VICTORY_CARD_BUFFERS_MB = '384'
    $settings.VICTORY_WINDOWS_BAR_MB = '0'
    $settings.VICTORY_TEXTURE_ROOM_PERCENT = '93'
    $settings.VICTORY_TEXTURE_STAGE_MB = '256'
    $settings.VICTORY_TEXTURE_STAGE_MS = '500'
    $settings.VICTORY_EARLY_TEXTURES_MS = '20'
    $settings.VICTORY_BAKE_TEXTURES_MS = '2500'
}

foreach ($k in $More.Keys) { $settings[$k] = $More[$k] }

if ($Build -eq 'auto') {
    $relPath = Join-Path $PSScriptRoot '..\target\release\victory.exe'
    $diagPath = Join-Path $PSScriptRoot '..\target\diag\victory.exe'
    if (Test-Path $relPath) { $Build = 'release' } elseif (Test-Path $diagPath) { $Build = 'diag' } else { $Build = 'release' }
}

if ($Build -eq 'diag' -and -not $settings.ContainsKey('VICTORY_D3D12_DRED')) {
    $settings.VICTORY_D3D12_DRED = 'faults'
}

$victory = Join-Path $PSScriptRoot "..\target\$Build\victory.exe"
if (-not (Test-Path $victory)) { Write-Host "No $victory. Build it first."; return }
if (-not (Test-Path $Exe)) { Write-Host "No $Exe."; return }
$disp = if ($settings.VICTORY_DISPLAY_MODE) { $settings.VICTORY_DISPLAY_MODE } else { 'default' }
$scaleShown = if ($settings.VICTORY_RENDER_SCALE) { $settings.VICTORY_RENDER_SCALE } else { 'default' }
$upShown = if ($settings.VICTORY_OWN_UPSCALER -eq '1') { 'own' } elseif ($SeriesX) { 'series-x' } else { 'fsr4' }
Write-Host "VIctory ($Build): NVIDIA VRAM=${vramMb}MB RAM=${ramGb}GB lowVram=$lowVram lowRam=$lowRam display=$disp scale=$scaleShown upscaler=$upShown"

if ($SmoothMotion) {
    $sm = Join-Path (Split-Path $victory) 'victory_sm.exe'
    if (-not (Test-Path $sm) -or (Get-Item $sm).LastWriteTime -lt (Get-Item $victory).LastWriteTime) {
        Copy-Item $victory $sm -Force
    }
    $victory = $sm
}

$paths = 'VICTORY_CONFIG', 'VICTORY_GAME_DIR', 'VICTORY_CACHE_DIR', 'VICTORY_DATA_DIR'
Get-ChildItem Env:VICTORY_* | Where-Object { $_.Name -notin $paths } | ForEach-Object { Remove-Item "Env:$($_.Name)" }
foreach ($k in $settings.Keys) { Set-Item "Env:$k" $settings[$k] }

$Exe = (Resolve-Path $Exe).Path
$runArgs = @('run', $Exe, '--execute')
if (-not $SeriesX) { $runArgs += @('--console', 'series-s') }

Push-Location (Join-Path $PSScriptRoot '..')   # victory.toml is looked for here
try {
    if ($Log) { & $victory @runArgs *> $Log } else { & $victory @runArgs }
} finally { Pop-Location }
