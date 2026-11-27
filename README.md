# VIctory-RTX3050-4gb

An NVIDIA 4 GB VRAM / 16 GB RAM laptop-tuned fork of [VIctory](https://github.com/Marvisak/VIctory), a compatibility layer that runs an Xbox Series S title natively on Windows from files the user supplies.

**This repository contains no game files, code or assets.** You need your own copy of the game. It is not affiliated with or endorsed by the game's developer or publisher.

## How to run it

Windows 10/11 x64, an AVX2 CPU, a Direct3D 12 GPU with Shader Model 6.6 and mesh shader support, the pinned Rust toolchain, and MSVC C++ build tools with a Windows SDK are required.

### Build

```powershell
cargo build --release --bin victory

# Optional diagnostics build:
cargo build --profile diag --bin victory --features diagnostics
```

### Launch

Using the NVIDIA laptop PowerShell launcher (auto-detects `<= 6 GB` VRAM and `<= 20 GB` RAM):

```powershell
.\scripts\play-nvidia.ps1 -Exe "D:\Games\MyDump\game_scarlett_master.exe"
```

Or using `Play-VIctory.cmd`:

```powershell
$env:VICTORY_GAME_DIR = 'D:\Games\MyDump'
.\Play-VIctory.cmd pareto-residency
```

Optional overrides on `scripts\play-nvidia.ps1`:
- `-RenderScale '0.67'` (720p internal) or `'1.0'` (1080p) for sharper shadows at lower FPS (default on 4 GB VRAM: `'0.5'` with `VICTORY_OWN_UPSCALER=1`).
- `-Build diag` to launch `target\diag\victory.exe`.
- `-RayTracing` to keep ray-tracing passes enabled (disabled by default via `scripts\rt_off_hashes.txt` and `VICTORY_BVH_MISS=1`).

## Verified Status (RTX 3050 Laptop 4 GB VRAM, 16 GB RAM)

- **Menus / loading screens**: `30 fps`
- **In-game world (1,400–3,500 draws/frame)**: `7–13 fps` (`75–134 ms/frame` CPU-bound in submission/refresh)

## License

GPL-3.0-or-later, as VIctory. See [LICENSE](LICENSE); the one file that is not
covered is named in [docs/asset-policy.md](docs/asset-policy.md).
