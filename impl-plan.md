# Implement `pgxtop` – a btop-style TUI for AI Workstations

Build a production-quality terminal application called **`pgxtop`**: a fast, visually polished **btop-style monitoring TUI optimized for NVIDIA AI workstations**, especially the NVIDIA/Lenovo PGX.

The goal is to replace the combination of `btop`, `nvtop`, `nvidia-smi`, `ollama ps` and manual vLLM monitoring with **one unified, beautiful terminal dashboard**.

## Core Technology

Prefer:

- **Rust**
- **Ratatui**
- `crossterm`
- NVIDIA **NVML** bindings
- `sysinfo` or equivalent for system metrics
- async HTTP via `reqwest` / `tokio`

Architecture must be modular so additional inference engines and agent runtimes can be added later.

## UX / Visual Design

The application must look and feel comparable to **btop**:

- modern TUI
- adaptive/responsive layout
- Unicode/Braille graphs
- sparklines
- live history charts
- progress bars
- 256-color / TrueColor support
- configurable themes
- keyboard navigation
- mouse support where useful
- sortable tables
- detail popups
- smooth refresh without flickering
- default refresh interval around 500 ms
- usable over SSH
- graceful degradation on smaller terminals

Example layout:

```text
╭─ PGXTOP ───────────────────────────────────────────────────────────╮
│ PGX ThinkStation             uptime 12d 04h              ● LIVE   │
╰───────────────────────────────────────────────────────────────────╯

╭─ GPU ──────────────────────────╮ ╭─ SYSTEM ───────────────────────╮
│ NVIDIA GPU                     │ │ CPU  ███████░ 32%              │
│ GPU   █████████████████░ 87%   │ │ RAM  █████████░ 174/512 GB    │
│ VRAM  ███████████████░ 108 GB │ │ Load 3.2 / 2.8 / 2.4          │
│ TEMP  61°C    POWER 428W       │ │                               │
│                               │ │ CPU HISTORY                    │
│ GPU HISTORY                   │ │ ⣀⣤⣿⣦⣀⣤⣿⣿⣦⣀     │
│ ⣀⣤⣿⣿⣿⣦⣀⣤⣿⣿    │ │                               │
╰───────────────────────────────╯ ╰───────────────────────────────╯

╭─ LLM ENGINES ─────────────────────────────────────────────────────╮
│ ● Ollama :11434                 ● vLLM :8888                      │
│                                                                    │
│ MODEL                     ENGINE    VRAM     CTX      STATUS        │
│ qwen3-coder-next:q8_0     Ollama    86 GB    64K      loaded        │
│ qwen3-235b-a22b           vLLM      21 GB    32K      active        │
╰────────────────────────────────────────────────────────────────────╯

╭─ INFERENCE ────────────────────╮ ╭─ GPU PROCESSES ────────────────╮
│ Requests/s       3.8           │ │ PID    PROCESS        VRAM     │
│ Prompt tok/s  2812             │ │ 18273  ollama         86 GB   │
│ Output tok/s   119             │ │ 19431  vllm           21 GB   │
│ Active            4            │ │                               │
│ Queue             2            │ │                               │
╰───────────────────────────────╯ ╰───────────────────────────────╯
```

Do not copy btop source code or visual assets. Recreate the UX concept independently.

---

# Functional Requirements

## 1. NVIDIA GPU Monitoring

Use NVML directly where possible.

Display:

- GPU model
- GPU utilization %
- VRAM used / total
- memory utilization
- temperature
- power usage
- power limit
- clocks
- fan speed if available
- PCIe throughput if available
- GPU processes
- process VRAM usage
- historical graphs for:
  - GPU utilization
  - VRAM
  - temperature
  - power

Support multiple GPUs cleanly even if the initial PGX has one GPU.

---

## 2. System Monitoring

Display:

- CPU utilization
- per-core utilization
- CPU history
- RAM
- swap
- system load
- uptime
- disk I/O
- network I/O
- process CPU/RAM consumption

Use efficient polling. The monitor itself must consume minimal CPU.

---

# 3. Ollama Integration

Auto-detect Ollama, default:

```text
http://localhost:11434
```

Use:

```text
GET /api/ps
```

Display all currently loaded models.

For every loaded model show as much information as available:

- model name
- model ID/digest
- model size
- VRAM allocation
- CPU/GPU split
- quantization
- context size
- expiration / keep-alive
- loaded status

Ollama being unavailable must **never crash pgxtop**.

Show states such as:

```text
● connected
○ unavailable
◐ connecting
```

---

# 4. vLLM Integration

Default endpoint:

```text
http://localhost:8888
```

Auto-detect OpenAI-compatible vLLM endpoints.

At minimum use:

```text
GET /v1/models
```

Additionally detect and consume vLLM Prometheus metrics where available.

Expose metrics such as:

- loaded model
- active requests
- waiting requests
- request throughput
- prompt tokens/sec
- generation tokens/sec
- KV cache utilization
- request latency
- time to first token
- tokens per request

Only show metrics that can actually be obtained. Never invent values.

Architecture should allow several vLLM instances.

---

# 5. Unified Model View

Create a central abstraction such as:

```rust
InferenceEngine
ModelInstance
InferenceMetrics
```

The UI should not care whether the model comes from:

- Ollama
- vLLM
- future llama.cpp
- future TensorRT-LLM
- future SGLang

Example unified table:

```text
MODEL                    ENGINE   VRAM      CONTEXT   STATUS
qwen3-coder-next:q8_0    Ollama   86.4 GB   64K       loaded
glm-5                    vLLM     72.1 GB   128K      active
```

---

# 6. Model ↔ GPU Process Correlation

Try to correlate inference engines/models with actual GPU processes.

For example:

```text
PID       PROCESS         ENGINE     MODEL                     VRAM
18273     ollama          Ollama     qwen3-coder-next:q8_0    86.4GB
19431     python          vLLM       qwen3-235b-a22b           21.3GB
```

Clearly distinguish between:

- confirmed mapping
- inferred mapping
- unknown mapping

Never present guesses as confirmed facts.

---

# 7. Views

Implement at least:

```text
[1] Overview
[2] GPU
[3] LLMs
[4] Processes
[5] System
[6] Network
```

### Overview

One-screen health overview.

### GPU

Detailed GPU telemetry and history.

### LLMs

Loaded models, inference engines and inference performance.

### Processes

System and GPU processes.

### System

CPU/RAM/disk.

### Network

Network throughput and active interfaces.

---

# 8. Keyboard Controls

At minimum:

```text
1-6       switch view
Tab       switch panel
↑ ↓       select
← →       change graph / column
Enter     details
s         sort
r         force refresh
p         pause
?         help
q         quit
Esc       close dialog
```

Use intuitive vim-style alternatives where practical:

```text
j k h l
```

---

# 9. Configuration

Support:

```text
~/.config/pgxtop/config.toml
```

Example:

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

Environment variables and CLI arguments should override config.

---

# 10. CLI

Support:

```bash
pgxtop
```

plus:

```bash
pgxtop --refresh 500

pgxtop --ollama http://localhost:11434

pgxtop --vllm http://localhost:8888

pgxtop --no-ollama

pgxtop --no-vllm

pgxtop --minimal

pgxtop --theme default
```

Provide:

```bash
pgxtop --help
pgxtop --version
```

---

# 11. Robustness

Important:

Individual monitoring providers must be isolated.

Examples:

- NVML unavailable → continue with system metrics
- Ollama unavailable → continue
- vLLM unavailable → continue
- malformed metrics response → continue
- network timeout → continue
- unsupported NVML feature → display N/A

The UI must never freeze because an HTTP call is slow.

Use asynchronous collectors and cached state.

Suggested architecture:

```text
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

Collectors should operate independently.

---

# 12. Performance

Target:

- UI refresh: 250–1000 ms configurable
- CPU overhead ideally <2%
- no busy loops
- bounded metric history
- minimal allocations in render loop
- no blocking HTTP inside render loop

Maintain e.g. 60–300 seconds of metric history depending on refresh rate.

---

# 13. Installation

Produce a normal Rust CLI binary.

Target primarily:

```text
Linux x86_64
Linux aarch64
```

Provide installation instructions:

```bash
cargo install --path .
```

and release binary workflow.

Prepare the repository so a future:

```bash
cargo install pgxtop
```

would be possible.

---

# 14. Documentation

Create:

```text
README.md
ARCHITECTURE.md
```

README should contain:

- purpose
- screenshots or terminal mockups
- installation
- configuration
- keybindings
- Ollama setup
- vLLM setup
- troubleshooting

Architecture documentation should explain:

- event loop
- collectors
- shared application state
- provider abstraction
- NVML integration
- metric history
- rendering architecture

---

# 15. Testing

Add meaningful tests for:

- Ollama response parsing
- vLLM model parsing
- Prometheus metric parsing
- config handling
- model normalization
- history buffers
- provider error handling

Use fixtures for mocked HTTP responses.

Do not require a physical NVIDIA GPU for the normal unit test suite.

---

# 16. Code Quality

Requirements:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features
cargo test
cargo build --release
```

must succeed.

Avoid:

- giant modules
- UI/business logic coupling
- `.unwrap()` in runtime paths
- blocking I/O in UI loop
- hardcoded PGX-specific assumptions
- fragile shell parsing where a proper API exists

---

# 17. Future Extensibility

Design extension points for:

- llama.cpp
- SGLang
- TensorRT-LLM
- additional Ollama instances
- remote AI hosts
- Claude Code sessions
- Codex sessions
- agent runtime monitoring
- token/cost accounting
- request/model attribution
- cluster/multi-node monitoring

Do **not** implement all of these now unless trivial. Design the interfaces so they can be added cleanly later.

---

# Implementation Strategy

Work autonomously.

1. Inspect the repository first.
2. If the repository is empty, initialize the project cleanly.
3. Design the architecture before creating large amounts of code.
4. Implement the minimum complete vertical slice:
   - event loop
   - system metrics
   - NVML
   - Ollama
   - basic TUI
5. Add vLLM.
6. Add history graphs and polished UX.
7. Add configuration and CLI.
8. Add tests.
9. Run all quality gates.
10. Fix all discovered issues.

Do not stop after scaffolding.

Do not deliver placeholder implementations for core functionality.

Use TODOs only for explicitly future functionality.

---

# Validation on the PGX

Where the local system provides the required services, verify against the real environment.

Check:

```bash
nvidia-smi
```

and:

```bash
curl http://localhost:11434/api/ps
```

and:

```bash
curl http://localhost:8888/v1/models
```

plus potential vLLM:

```bash
curl http://localhost:8888/metrics
```

Detect actual service capabilities instead of assuming them.

Do not modify or restart Ollama/vLLM merely to make the monitor work.

---

# Definition of Done

The implementation is done when I can SSH into the PGX and execute:

```bash
pgxtop
```

and immediately get a polished btop-like dashboard showing:

- GPU utilization
- GPU VRAM
- GPU temperature/power
- GPU history
- CPU
- RAM
- currently loaded Ollama models
- available/loaded vLLM models
- inference metrics where available
- GPU processes
- model/process correlation
- responsive keyboard navigation

The experience should feel like:

> **btop for an AI workstation**

Prioritize:

1. correctness
2. visual quality
3. robustness
4. usability
5. performance

Use all available Codex capabilities to inspect, implement, build, test and improve the application. Continue iterating until the repository contains a working, polished implementation rather than merely a prototype.