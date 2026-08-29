pub mod ollama;
pub mod vllm;

pub use crate::models::inference::{EngineStatus, EngineType, InferenceEngine};

/// Why a probe failed.
///
/// Typed rather than collapsed into a single `Unavailable`, so the engines row
/// can say *why* — "connection refused" and "HTTP 404 from something that is
/// not Ollama" need very different fixes from the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    /// Connection refused, DNS failure, timeout.
    Transport(String),
    /// Reached a server, but it answered with an error status.
    Status(u16),
    /// 200 OK, but the body did not parse as the expected shape.
    Malformed(String),
}

impl ProbeError {
    /// Short, single-line rendering for the engines row.
    pub fn short(&self) -> String {
        match self {
            Self::Transport(e) => {
                // reqwest's Display is long and wraps the whole URL chain.
                let e = e.split(':').next_back().unwrap_or(e).trim();
                if e.is_empty() {
                    "unreachable".to_string()
                } else {
                    e.to_string()
                }
            }
            Self::Status(code) => format!("HTTP {code}"),
            Self::Malformed(_) => "bad response".to_string(),
        }
    }
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "transport: {e}"),
            Self::Status(c) => write!(f, "http status {c}"),
            Self::Malformed(e) => write!(f, "malformed response: {e}"),
        }
    }
}
