use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "pgxtop", version, about = "A btop-style TUI for NVIDIA AI workstations")]
pub struct Cli {
    /// Refresh interval in milliseconds
    #[arg(long, default_value = "500")]
    pub refresh: u64,

    /// Ollama endpoint URL
    #[arg(long)]
    pub ollama: Option<String>,

    /// vLLM endpoint URL
    #[arg(long)]
    pub vllm: Option<String>,

    /// Disable Ollama integration
    #[arg(long)]
    pub no_ollama: bool,

    /// Disable vLLM integration
    #[arg(long)]
    pub no_vllm: bool,

    /// Minimal mode (reduced UI)
    #[arg(long)]
    pub minimal: bool,

    /// Theme name
    #[arg(long)]
    pub theme: Option<String>,
}