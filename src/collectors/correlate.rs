//! Attributing GPU processes to inference engines and to individual models.
//!
//! `impl-plan.md` §6 is explicit: "Never present guesses as confirmed facts."
//! Every mapping therefore carries a [`MappingConfidence`], and the UI renders
//! the marker rather than silently presenting a heuristic as a fact.
//!
//! Everything here works off `/proc/<pid>/cmdline`. Measured on the target
//! host, an Ollama runner looks like:
//!
//! ```text
//! /usr/local/lib/ollama/llama-server
//!   --model /usr/share/ollama/.ollama/models/blobs/sha256-cccc9999dddd...
//!   --port 34471 --host 127.0.0.1 -c 262144 ...
//! ```
//!
//! Two things follow. `comm` is `llama-server`, not `ollama`, so matching on
//! the short name alone does not identify the engine. And the digest in the
//! cmdline is the *blob* digest of the weights, while `/api/ps` reports the
//! *manifest* digest — they are different values, so they cannot be compared
//! directly. [`ManifestIndex`] bridges the two.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::models::{GpuProcess, MappingConfidence, ModelInstance};

pub const ENGINE_OLLAMA: &str = "Ollama";
pub const ENGINE_VLLM: &str = "vLLM";
pub const ENGINE_LLAMA_CPP: &str = "llama.cpp";

/// Identifies the engine from a process's argv, falling back to its short name.
pub fn engine_of(cmdline: Option<&str>, comm: &str) -> Option<&'static str> {
    let hay = cmdline.unwrap_or(comm).to_ascii_lowercase();
    let argv0 = hay.split_whitespace().next().unwrap_or(&hay);

    if argv0.contains("/ollama/") || argv0.contains("ollama") || hay.contains(".ollama/models") {
        return Some(ENGINE_OLLAMA);
    }
    if hay.contains("vllm") {
        return Some(ENGINE_VLLM);
    }
    if argv0.contains("llama-server") || argv0.contains("llama.cpp") || argv0.contains("llama-cli")
    {
        return Some(ENGINE_LLAMA_CPP);
    }
    None
}

/// Extracts the GGUF blob digest an Ollama runner was started with.
pub fn blob_digest_of(cmdline: &str) -> Option<String> {
    let idx = cmdline.find("/blobs/sha256-")?;
    let rest = &cmdline[idx + "/blobs/".len()..];
    let end = rest
        .find(|c: char| c.is_whitespace())
        .unwrap_or(rest.len());
    let digest = &rest[..end];
    if digest.len() > "sha256-".len() {
        Some(digest.to_string())
    } else {
        None
    }
}

/// Extracts the loaded context size from `-c N` / `--ctx-size N`.
pub fn context_of(cmdline: &str) -> Option<u32> {
    let mut it = cmdline.split_whitespace();
    while let Some(tok) = it.next() {
        if tok == "-c" || tok == "--ctx-size" {
            if let Some(v) = it.next() {
                return v.parse().ok();
            }
        }
        if let Some(v) = tok.strip_prefix("--ctx-size=") {
            return v.parse().ok();
        }
    }
    None
}

// ---------------------------------------------------------------------------
// blob digest -> model name
// ---------------------------------------------------------------------------

/// Maps a weights blob digest to the model name Ollama displays for it, by
/// reading the manifest tree under the Ollama models directory.
///
/// Verified readable by an unprivileged user on the target host. When it is
/// not readable the index is simply empty and correlation falls through to the
/// heuristics below — no error surfaces to the user.
#[derive(Debug, Default, Clone)]
pub struct ManifestIndex {
    by_blob: HashMap<String, String>,
}

impl ManifestIndex {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.by_blob.is_empty()
    }

    pub fn len(&self) -> usize {
        self.by_blob.len()
    }

    pub fn lookup(&self, blob_digest: &str) -> Option<&str> {
        self.by_blob.get(blob_digest).map(String::as_str)
    }

    /// Candidate Ollama model roots, most specific first.
    pub fn default_roots() -> Vec<PathBuf> {
        let mut roots = Vec::new();
        if let Ok(p) = std::env::var("OLLAMA_MODELS") {
            roots.push(PathBuf::from(p));
        }
        roots.push(PathBuf::from("/usr/share/ollama/.ollama/models"));
        if let Some(home) = dirs::home_dir() {
            roots.push(home.join(".ollama/models"));
        }
        roots.push(PathBuf::from("/var/lib/ollama/.ollama/models"));
        roots
    }

    pub fn discover() -> Self {
        for root in Self::default_roots() {
            let idx = Self::from_models_dir(&root);
            if !idx.is_empty() {
                tracing::debug!(
                    target: "pgxtop::correlate",
                    "manifest index built from {}: {} entries", root.display(), idx.len()
                );
                return idx;
            }
        }
        Self::empty()
    }

    pub fn from_models_dir(models_dir: &Path) -> Self {
        let manifests = models_dir.join("manifests");
        let mut by_blob = HashMap::new();
        let mut files = Vec::new();
        collect_files(&manifests, 0, &mut files);

        for path in files {
            let Ok(rel) = path.strip_prefix(&manifests) else {
                continue;
            };
            let Some(name) = display_name(rel) else {
                continue;
            };
            let Ok(body) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) else {
                continue;
            };
            if let Some(digest) = model_layer_digest(&json) {
                // Ollama stores blobs as `sha256-<hex>` while manifests spell
                // the same value `sha256:<hex>`.
                by_blob.insert(digest.replace(':', "-"), name);
            }
        }

        Self { by_blob }
    }
}

fn model_layer_digest(manifest: &serde_json::Value) -> Option<String> {
    manifest
        .get("layers")?
        .as_array()?
        .iter()
        .find(|l| {
            l.get("mediaType").and_then(|m| m.as_str())
                == Some("application/vnd.ollama.image.model")
        })?
        .get("digest")?
        .as_str()
        .map(str::to_string)
}

/// `registry.ollama.ai/library/llama3.1/70b` -> `llama3.1:70b`
/// `registry.ollama.ai/acme/Qwen3-27B/q8_0` -> `acme/Qwen3-27B:q8_0`
fn display_name(rel: &Path) -> Option<String> {
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    // registry / namespace... / name / tag
    if parts.len() < 4 {
        return None;
    }
    let tag = parts.last()?;
    let name = parts.get(parts.len() - 2)?;
    let namespace = parts[1..parts.len() - 2].join("/");
    if namespace == "library" {
        Some(format!("{name}:{tag}"))
    } else {
        Some(format!("{namespace}/{name}:{tag}"))
    }
}

fn collect_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 6 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => collect_files(&path, depth + 1, out),
            Ok(ft) if ft.is_file() => out.push(path),
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// the correlation ladder
// ---------------------------------------------------------------------------

/// Fills `engine`, `model` and `confidence` on each process.
///
/// Descending certainty:
///  1. blob digest from the cmdline resolved through the manifest index
///  2. exactly one candidate process and exactly one loaded model
///  3. `-c N` matching a unique loaded `context_size`
///  4. the process footprint being the tightest fit at or above `size_vram`
///  5. engine only, model unknown
pub fn correlate(
    processes: &mut [GpuProcess],
    models: &[ModelInstance],
    manifests: &ManifestIndex,
) {
    for p in processes.iter_mut() {
        p.engine = engine_of(p.cmdline.as_deref(), &p.name).map(str::to_string);
        p.model = None;
        p.confidence = MappingConfidence::Unknown;
    }

    // Step 1 — confirmed via the manifest index.
    if !manifests.is_empty() {
        for p in processes.iter_mut() {
            let Some(cmdline) = p.cmdline.as_deref() else {
                continue;
            };
            let Some(blob) = blob_digest_of(cmdline) else {
                continue;
            };
            if let Some(name) = manifests.lookup(&blob) {
                p.model = Some(name.to_string());
                p.confidence = MappingConfidence::Confirmed;
            }
        }
    }

    // The remaining steps only consider resident models and processes that are
    // still unattributed.
    let resident: Vec<&ModelInstance> = models.iter().filter(|m| m.status.is_resident()).collect();

    let claimed: Vec<String> = processes.iter().filter_map(|p| p.model.clone()).collect();
    let mut unclaimed: Vec<&ModelInstance> = resident
        .iter()
        .copied()
        .filter(|m| !claimed.contains(&m.name))
        .collect();

    let pending: Vec<usize> = processes
        .iter()
        .enumerate()
        .filter(|(_, p)| p.model.is_none() && p.engine.is_some())
        .map(|(i, _)| i)
        .collect();

    // Step 2 — one process, one model: unambiguous.
    if pending.len() == 1 && unclaimed.len() == 1 {
        let i = pending[0];
        processes[i].model = Some(unclaimed[0].name.clone());
        processes[i].confidence = MappingConfidence::Confirmed;
        return;
    }

    // Step 3 — a uniquely matching context size.
    for &i in &pending {
        if processes[i].model.is_some() {
            continue;
        }
        let Some(ctx) = processes[i].cmdline.as_deref().and_then(context_of) else {
            continue;
        };
        let matches: Vec<&ModelInstance> = unclaimed
            .iter()
            .copied()
            .filter(|m| m.context_size == Some(ctx))
            .collect();
        if matches.len() == 1 {
            let name = matches[0].name.clone();
            processes[i].model = Some(name.clone());
            processes[i].confidence = MappingConfidence::Inferred;
            unclaimed.retain(|m| m.name != name);
        }
    }

    // Step 4 — tightest footprint at or above the model's VRAM residency.
    // A runner always holds at least the weights, plus KV cache on top; on the
    // measured host that gap was ~10 GiB, so the fit is deliberately loose in
    // one direction only.
    for &i in &pending {
        if processes[i].model.is_some() {
            continue;
        }
        let Some(used) = processes[i].used_memory else {
            continue;
        };
        let best = unclaimed
            .iter()
            .copied()
            .filter_map(|m| m.size_vram.map(|v| (m, v)))
            .filter(|(_, v)| *v > 0 && used >= *v)
            .min_by_key(|(_, v)| used - v);
        if let Some((m, _)) = best {
            let name = m.name.clone();
            processes[i].model = Some(name.clone());
            processes[i].confidence = MappingConfidence::Inferred;
            unclaimed.retain(|m| m.name != name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::inference::{ModelStatus, ProcessorSplit};

    /// The exact command line measured on the PGX.
    const OLLAMA_CMDLINE: &str = "/usr/local/lib/ollama/llama-server --model /usr/share/ollama/.ollama/models/blobs/sha256-cccc9999dddd0000eeee1111ffff2222aaaa3333bbbb4444cccc5555dddd6666 --port 34471 --host 127.0.0.1 --no-webui --offline -c 262144 -np 1 --load-mode dio --flash-attn auto";

    fn gproc(pid: u32, name: &str, cmdline: Option<&str>, mem: Option<u64>) -> GpuProcess {
        GpuProcess {
            pid,
            name: name.into(),
            cmdline: cmdline.map(str::to_string),
            gpu_index: 0,
            used_memory: mem,
            graphics: false,
            engine: None,
            model: None,
            confidence: MappingConfidence::Unknown,
        }
    }

    fn model(name: &str, vram: Option<u64>, ctx: Option<u32>) -> ModelInstance {
        ModelInstance {
            id: format!("ollama/{name}"),
            name: name.into(),
            engine_id: "ollama".into(),
            size_total: vram,
            size_vram: vram,
            processor: Some(ProcessorSplit::AllGpu),
            context_size: ctx,
            status: ModelStatus::Loaded,
            ..Default::default()
        }
    }

    /// `comm` is `llama-server`, so the short name alone must not be trusted —
    /// the engine has to come out of argv[0].
    #[test]
    fn engine_detected_from_argv0_not_from_comm() {
        assert_eq!(engine_of(Some(OLLAMA_CMDLINE), "llama-server"), Some(ENGINE_OLLAMA));
        // Same binary name, no Ollama in the path: a plain llama.cpp server.
        assert_eq!(
            engine_of(Some("/opt/llama.cpp/llama-server -m /models/x.gguf"), "llama-server"),
            Some(ENGINE_LLAMA_CPP)
        );
        assert_eq!(
            engine_of(Some("/usr/bin/python3 -m vllm.entrypoints.openai.api_server"), "python3"),
            Some(ENGINE_VLLM)
        );
        assert_eq!(engine_of(None, "chrome"), None);
    }

    #[test]
    fn blob_digest_and_context_are_extracted_from_the_real_cmdline() {
        assert_eq!(
            blob_digest_of(OLLAMA_CMDLINE).as_deref(),
            Some("sha256-cccc9999dddd0000eeee1111ffff2222aaaa3333bbbb4444cccc5555dddd6666")
        );
        assert_eq!(context_of(OLLAMA_CMDLINE), Some(262_144));
        assert_eq!(blob_digest_of("/usr/bin/python3"), None);
        assert_eq!(context_of("--ctx-size=8192"), Some(8192));
    }

    #[test]
    fn manifest_path_becomes_the_display_name() {
        assert_eq!(
            display_name(Path::new("registry.ollama.ai/library/llama3.1/70b")).as_deref(),
            Some("llama3.1:70b")
        );
        assert_eq!(
            display_name(Path::new("registry.ollama.ai/acme/example-27b/q8_0")).as_deref(),
            Some("acme/example-27b:q8_0")
        );
        assert_eq!(display_name(Path::new("too/short")), None);
    }

    #[test]
    fn manifest_index_maps_blob_digest_to_model_name() {
        let dir = std::env::temp_dir().join(format!("pgxtop-manifest-{}", std::process::id()));
        let leaf = dir.join("manifests/registry.ollama.ai/library/example-moe");
        std::fs::create_dir_all(&leaf).unwrap();
        std::fs::write(
            leaf.join("q4_K_M"),
            r#"{"schemaVersion":2,"layers":[
                 {"mediaType":"application/vnd.ollama.image.license","digest":"sha256:aaa"},
                 {"mediaType":"application/vnd.ollama.image.model","digest":"sha256:cccc9999dddd"}
               ]}"#,
        )
        .unwrap();

        let idx = ManifestIndex::from_models_dir(&dir);
        assert_eq!(idx.len(), 1);
        assert_eq!(idx.lookup("sha256-cccc9999dddd"), Some("example-moe:q4_K_M"));
        // The license layer must not be indexed.
        assert_eq!(idx.lookup("sha256-aaa"), None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_manifest_dir_yields_an_empty_index_not_an_error() {
        let idx = ManifestIndex::from_models_dir(Path::new("/nonexistent/pgxtop/models"));
        assert!(idx.is_empty());
    }

    #[test]
    fn manifest_hit_is_confirmed() {
        let mut idx = ManifestIndex::empty();
        idx.by_blob.insert(
            "sha256-cccc9999dddd0000eeee1111ffff2222aaaa3333bbbb4444cccc5555dddd6666".into(),
            "example-moe:q8_0".into(),
        );

        let mut procs = vec![
            gproc(245034, "llama-server", Some(OLLAMA_CMDLINE), Some(109_000_000_000)),
            gproc(3449, "python", Some("/usr/bin/python3 train.py"), Some(178_000_000)),
        ];
        let models = vec![
            model("example-moe:q8_0", Some(96_261_027_921), Some(262_144)),
            model("nomic-embed-text:latest", Some(274_000_000), Some(8192)),
        ];

        correlate(&mut procs, &models, &idx);

        assert_eq!(procs[0].engine.as_deref(), Some(ENGINE_OLLAMA));
        assert_eq!(procs[0].model.as_deref(), Some("example-moe:q8_0"));
        assert_eq!(procs[0].confidence, MappingConfidence::Confirmed);

        // A plain python job is neither an engine nor a model.
        assert_eq!(procs[1].engine, None);
        assert_eq!(procs[1].model, None);
        assert_eq!(procs[1].confidence, MappingConfidence::Unknown);
    }

    #[test]
    fn single_process_single_model_is_confirmed_without_a_manifest() {
        let mut procs = vec![gproc(1, "llama-server", Some(OLLAMA_CMDLINE), Some(109_000_000_000))];
        let models = vec![model("example-moe:q8_0", Some(96_261_027_921), Some(262_144))];
        correlate(&mut procs, &models, &ManifestIndex::empty());
        assert_eq!(procs[0].model.as_deref(), Some("example-moe:q8_0"));
        assert_eq!(procs[0].confidence, MappingConfidence::Confirmed);
    }

    #[test]
    fn context_size_disambiguates_two_runners() {
        let mut procs = vec![
            gproc(1, "llama-server", Some("/usr/local/lib/ollama/llama-server -c 262144"), Some(100)),
            gproc(2, "llama-server", Some("/usr/local/lib/ollama/llama-server -c 8192"), Some(100)),
        ];
        let models = vec![
            model("big:q8", Some(90), Some(262_144)),
            model("small:f16", Some(10), Some(8192)),
        ];
        correlate(&mut procs, &models, &ManifestIndex::empty());
        assert_eq!(procs[0].model.as_deref(), Some("big:q8"));
        assert_eq!(procs[0].confidence, MappingConfidence::Inferred);
        assert_eq!(procs[1].model.as_deref(), Some("small:f16"));
        assert_eq!(procs[1].confidence, MappingConfidence::Inferred);
    }

    #[test]
    fn footprint_fit_is_the_last_resort_and_stays_inferred() {
        let mut procs = vec![
            gproc(1, "llama-server", Some("/usr/local/lib/ollama/llama-server"), Some(95)),
            gproc(2, "llama-server", Some("/usr/local/lib/ollama/llama-server"), Some(15)),
        ];
        let models = vec![
            model("big:q8", Some(90), None),
            model("small:f16", Some(10), None),
        ];
        correlate(&mut procs, &models, &ManifestIndex::empty());
        assert_eq!(procs[0].model.as_deref(), Some("big:q8"));
        assert_eq!(procs[1].model.as_deref(), Some("small:f16"));
        assert!(procs.iter().all(|p| p.confidence == MappingConfidence::Inferred));
    }

    /// Ambiguity must stay ambiguous rather than be resolved by a coin flip.
    #[test]
    fn ambiguous_runners_stay_unknown() {
        let mut procs = vec![
            gproc(1, "llama-server", Some("/usr/local/lib/ollama/llama-server"), None),
            gproc(2, "llama-server", Some("/usr/local/lib/ollama/llama-server"), None),
        ];
        let models = vec![model("a:q8", Some(10), None), model("b:q8", Some(10), None)];
        correlate(&mut procs, &models, &ManifestIndex::empty());
        for p in &procs {
            assert_eq!(p.engine.as_deref(), Some(ENGINE_OLLAMA));
            assert_eq!(p.model, None);
            assert_eq!(p.confidence, MappingConfidence::Unknown);
        }
    }

    /// Installed-but-not-loaded models are not candidates for attribution.
    #[test]
    fn installed_only_models_are_not_attributed() {
        let mut procs = vec![gproc(1, "llama-server", Some("/usr/local/lib/ollama/llama-server"), Some(50))];
        let mut installed = model("catalog-only:q4", Some(40), None);
        installed.status = ModelStatus::Installed;
        correlate(&mut procs, &[installed], &ManifestIndex::empty());
        assert_eq!(procs[0].model, None);
        assert_eq!(procs[0].confidence, MappingConfidence::Unknown);
    }
}
