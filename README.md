# pgxtop

A fast, visually polished **btop-style TUI for NVIDIA AI workstations**, especially the NVIDIA/Lenovo PGX.

## Features

- **GPU Monitoring**: NVIDIA GPU utilization, VRAM, temperature, power, fan speed, PCIe throughput
- **System Monitoring**: CPU, RAM, swap, load average, disk I/O, network I/O, processes
- **Ollama Integration**: every column `ollama ps` shows — name, ID, size, CPU/GPU split, context, keep-alive — plus the installed catalogue from `/api/tags`
- **Model Details**: `Enter` opens a per-model panel with quantization, architecture, capabilities, loaded-vs-maximum context and the GPU processes holding the memory
- **vLLM Integration**: `/v1/models` plus Prometheus metrics, with throughput derived from the token counters
- **Unified Model View**: one table for all inference engines (Ollama, vLLM, future llama.cpp, TensorRT-LLM, SGLang)
- **GPU Process Correlation**: maps runners to models, marking each mapping confirmed, inferred or unknown
- **Unified Memory Aware**: on Grace-Blackwell (GB10) NVML reports no frame buffer, so pgxtop reports shared host memory and GPU residency instead of a fabricated `0/0 GB`
- **Multiple Views**: Overview, GPU, Models, Processes, System, Network
- **Beautiful TUI**: Unicode graphs, progress bars, threshold colouring
- **Keyboard Navigation**: vim-style controls, sortable table, detail overlay
- **Robust**: engines polled in a background task, so a dead endpoint never freezes the UI

## Installation

```bash
cargo install --path .
```

Or from crates.io (when published):

```bash
cargo install pgxtop
```

## Usage

```bash
pgxtop
```

### CLI Options

```bash
pgxtop --refresh 500          # Render/NVML refresh interval (ms); engines poll separately
pgxtop --ollama http://localhost:11434
pgxtop --vllm http://localhost:8888
pgxtop --no-ollama            # Disable Ollama
pgxtop --no-vllm              # Disable vLLM
pgxtop --minimal              # Minimal mode
pgxtop --theme default        # Set theme
pgxtop --help                 # Show help
pgxtop --version              # Show version
```

## Configuration

Config file: `~/.config/pgxtop/config.toml`

Every key is optional — anything you leave out falls back to the default below.

```toml
# Render, NVML and sysinfo cadence.
refresh_ms = 500

[ollama]
enabled = true
url = "http://localhost:11434"
show_details = true       # fetch /api/show for the selected model (on Enter, cached)
include_installed = true  # also list installed-but-not-loaded models from /api/tags

[[vllm]]
name = "local"
url = "http://localhost:8888"

# Engine polling, deliberately slower than the render loop: `ollama ps`
# changes on the order of minutes.
[inference]
refresh_ms = 2000
tags_refresh_ms = 30000
timeout_ms = 1500
connect_timeout_ms = 400
show_timeout_ms = 3000
stale_after_ms = 10000    # beyond this a model row is dimmed and shows its age
drop_after_ms = 60000     # beyond this the retained rows are dropped

[ui]
theme = "default"
graphs = true
mouse = false
```

## Keybindings

Global:

| Key | Action |
|-----|--------|
| 1-6 | Switch view |
| r | Force refresh now (works while paused) |
| p | Pause / resume collection |
| ? | Help overlay |
| q / Ctrl-C | Quit |
| Esc | Close overlay |

Models view (`3`):

| Key | Action |
|-----|--------|
| ↑ ↓ / j k | Select model |
| PgUp / PgDn | Page |
| g / G, Home / End | First / last row |
| Enter | Detail overlay (loads `/api/show` once, then cached) |
| s | Cycle sort column |
| S | Reverse sort |
| Tab | Cycle engine filter (only with more than one engine) |
| t | Show/hide the inference throughput graphs |

The footer is context-sensitive and only advertises keys that actually do
something in the current view.

## Views

1. **Overview** - One-screen health overview
2. **GPU** - Detailed GPU telemetry and history
3. **Models** - GPU strip, engine status, the full model table and the detail overlay
4. **Processes** - System and GPU processes
5. **System** - CPU/RAM/disk
6. **Network** - Network throughput and active interfaces

## Ollama Setup

Install Ollama:

```bash
curl -fsSL https://ollama.com/install.sh | sh
```

Start Ollama:

```bash
ollama serve
```

## vLLM Setup

Install vLLM:

```bash
pip install vllama
```

Start vLLM:

```bash
vllm serve <model> --port 8888
```

## Troubleshooting

- **NVML unavailable**: GPU panels say so and name the reason; system metrics still work
- **Unsupported NVML fields**: shown as `N/A`, never as `0`. On a GB10 that covers `memory.used`/`memory.total`, `power.limit`, `clocks.mem` and `fan.speed`
- **Ollama or vLLM unavailable**: the last known models stay visible, dimmed with their age, and are dropped after `drop_after_ms`. The reason is shown next to the engine (`HTTP 404`, `connection refused`, ...)
- **Slow HTTP**: engines are polled in a background task, so the UI never blocks on a request
- **`config.toml` ignored**: a parse error is logged to stderr — run with `2>/tmp/pgxtop.log` to see it

## License

MIT