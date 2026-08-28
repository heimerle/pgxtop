# pgxtop

A fast, visually polished **btop-style TUI for NVIDIA AI workstations**, especially the NVIDIA/Lenovo PGX.

## Features

- **GPU Monitoring**: NVIDIA GPU utilization, VRAM, temperature, power, fan speed, PCIe throughput
- **System Monitoring**: CPU, RAM, swap, load average, disk I/O, network I/O, processes
- **Ollama Integration**: Auto-detects Ollama, shows loaded models with VRAM usage
- **vLLM Integration**: Auto-detects vLLM endpoints, shows models and Prometheus metrics
- **Unified Model View**: Single table for all inference engines (Ollama, vLLM, future llama.cpp, TensorRT-LLM, SGLang)
- **GPU Process Correlation**: Maps inference engines to actual GPU processes
- **Multiple Views**: Overview, GPU, LLMs, Processes, System, Network
- **Beautiful TUI**: Unicode/Braille graphs, sparklines, progress bars, 256-color/TrueColor support
- **Keyboard Navigation**: Vim-style controls, sortable tables, detail popups
- **Robust**: Isolated collectors, graceful degradation, async HTTP, no UI freezing

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
pgxtop --refresh 500          # Set refresh interval (ms)
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

```toml
refresh_ms = 500

[ollama]
enabled = true
url = "http://localhost:11434"

[[vllm]]
name = "local"
url = "http://localhost:8888"

[ui]
theme = "default"
graphs = true
mouse = true
```

## Keybindings

| Key | Action |
|-----|--------|
| 1-6 | Switch view |
| Tab | Switch panel |
| ↑ ↓ / j k | Select |
| ← → / h l | Change graph/column |
| Enter | Details |
| s | Sort |
| r | Force refresh |
| p | Pause |
| ? | Help |
| q | Quit |
| Esc | Close dialog |

## Views

1. **Overview** - One-screen health overview
2. **GPU** - Detailed GPU telemetry and history
3. **LLMs** - Loaded models, inference engines and performance
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

- **NVML unavailable**: GPU monitoring will be disabled, system metrics still work
- **Ollama unavailable**: pgxtop continues without Ollama integration
- **vLLM unavailable**: pgxtop continues without vLLM integration
- **Slow HTTP**: All HTTP calls have timeouts and run asynchronously

## License

MIT