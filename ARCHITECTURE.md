# Architecture

## Overview

pgxtop is a terminal-based monitoring dashboard for NVIDIA AI workstations. It uses a modular architecture with isolated collectors, shared application state, and a Ratatui-based UI.

## Architecture Diagram

```
┌──────────────────────┐
│      Ratatui UI      │
└──────────┬───────────┘
           │
        AppState
           │
 ┌─────────┼──────────────┐
 │         │              │
NVML     System       Inference
                     Providers
                    ┌────┴────┐
                  Ollama    vLLM
```

## Event Loop

The main event loop runs in `src/app.rs`:

1. Check if refresh interval has elapsed
2. If not paused, collect data from all collectors
3. Render the UI
4. Poll for keyboard events
5. Handle key events (view switching, navigation, etc.)
6. Repeat

## Collectors

Collectors are isolated modules that gather data independently:

- **NVML Collector** (`src/collectors/nvml.rs`): GPU metrics via NVML
- **System Collector** (`src/collectors/system.rs`): CPU, RAM, disk, network via `sysinfo`
- **Inference Collector** (`src/collectors/inference.rs`): Ollama and vLLM metrics via HTTP

Each collector can fail independently without affecting others.

## Shared Application State

`AppState` (in `src/app.rs`) holds all collected data:

- GPU info, metrics, processes, and history
- System info, metrics, processes, and history
- Inference engines, model instances, metrics, and history

State is updated by collectors and read by the UI renderer.

## Provider Abstraction

Inference engines implement a common interface:

```rust
InferenceEngine  // Engine metadata (type, URL, status)
ModelInstance    // Model metadata (name, VRAM, status)
InferenceMetrics // Runtime metrics (throughput, tokens/s, etc.)
```

New engines (llama.cpp, TensorRT-LLM, SGLang) can be added by implementing the same interface.

## NVML Integration

NVML is accessed via the `nvml` crate. The collector:

1. Initializes NVML
2. Enumerates GPU devices
3. Collects utilization, memory, temperature, power, clocks, fan speed
4. Collects GPU processes and their VRAM usage

If NVML is unavailable, the collector returns empty data and the UI degrades gracefully.

## Metric History

Metrics are stored in bounded history buffers:

- `GpuHistory`: GPU utilization, VRAM, temperature, power (300 points)
- `SystemHistory`: CPU, memory (300 points)
- `InferenceHistory`: Prompt tokens/s, generation tokens/s, active requests (300 points)

History is trimmed to `max_points` to prevent unbounded memory growth.

## Rendering Architecture

The UI is organized into views:

- `src/ui/mod.rs`: Main UI controller, header/footer rendering
- `src/ui/views/`: Individual view implementations
- `src/ui/widgets/`: Reusable widgets (graph, sparkline, table)

Each view renders its own section of the terminal using Ratatui's layout system.

## Async Architecture

HTTP calls (Ollama, vLLM) run asynchronously via `tokio` and `reqwest`. All HTTP calls have timeouts to prevent UI freezing.

## Configuration

Configuration is loaded from:

1. `~/.config/pgxtop/config.toml` (file)
2. CLI arguments (override file)
3. Environment variables (future)

## Error Handling

- Individual collectors are isolated
- HTTP errors are caught and logged
- NVML errors are caught and logged
- The UI never crashes due to collector failures