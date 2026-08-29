//! Background inference poller.
//!
//! The render loop must never await an HTTP call: previously `App::refresh`
//! awaited `collect()` inline, which probed engines sequentially with a 5 s
//! timeout each, so two unreachable engines froze the whole TUI — input
//! included — for up to ~20 s per tick.
//!
//! Here a tokio task owns every client, probes all engines concurrently and
//! publishes an immutable snapshot through a `watch` channel. Reading it from
//! the render loop is an `Arc` refcount bump.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;

use crate::engines::ollama::show_cache::{Lookup, ShowCache, ShowKey};
use crate::engines::ollama::OllamaClient;
use crate::engines::vllm::{Counters, VllmClient};
use crate::engines::{EngineStatus, EngineType, InferenceEngine, ProbeError};
use crate::models::inference::{InferenceMetrics, ModelInstance};

#[derive(Debug, Clone)]
pub struct InferenceConfig {
    pub refresh_ms: u64,
    pub tags_refresh_ms: u64,
    pub timeout_ms: u64,
    pub connect_timeout_ms: u64,
    pub show_timeout_ms: u64,
    /// Beyond this age a snapshot is rendered dimmed with its age.
    pub stale_after_ms: u64,
    /// Beyond this age the retained models are dropped — a genuinely dead
    /// engine must not show phantom models forever.
    pub drop_after_ms: u64,
    pub show_details: bool,
    pub include_installed: bool,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            refresh_ms: 2000,
            tags_refresh_ms: 30_000,
            timeout_ms: 1500,
            connect_timeout_ms: 400,
            show_timeout_ms: 3000,
            stale_after_ms: 10_000,
            drop_after_ms: 60_000,
            show_details: true,
            include_installed: true,
        }
    }
}

/// One engine's last known state.
///
/// `models` and `last_ok` are only replaced on success. A failed probe changes
/// the status and the error, nothing else — so a single dropped packet no
/// longer blanks the table and resets the user's selection.
#[derive(Debug, Clone)]
pub struct EngineSnapshot {
    pub engine: InferenceEngine,
    pub models: Vec<ModelInstance>,
    pub metrics: Option<InferenceMetrics>,
    pub last_ok: Option<Instant>,
    pub consecutive_failures: u32,
    pub last_error: Option<ProbeError>,
}

impl EngineSnapshot {
    pub fn age(&self) -> Option<Duration> {
        self.last_ok.map(|t| t.elapsed())
    }

    pub fn is_stale(&self, stale_after: Duration) -> bool {
        match self.last_ok {
            Some(t) => t.elapsed() > stale_after,
            // Never reached: not "stale", just unavailable.
            None => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InferenceSnapshot {
    pub engines: Vec<EngineSnapshot>,
    /// Bumped once per completed poll cycle. The App uses it to avoid pushing
    /// the same metrics sample into history on every 500 ms render tick.
    pub generation: u64,
    pub stale_after: Duration,
}

impl Default for InferenceSnapshot {
    fn default() -> Self {
        Self {
            engines: Vec::new(),
            generation: 0,
            stale_after: Duration::from_millis(10_000),
        }
    }
}

impl InferenceSnapshot {
    /// Every model across every engine, flattened.
    pub fn models(&self) -> impl Iterator<Item = &ModelInstance> {
        self.engines.iter().flat_map(|e| e.models.iter())
    }

    pub fn any_connected(&self) -> bool {
        self.engines
            .iter()
            .any(|e| e.engine.status == EngineStatus::Connected)
    }
}

#[derive(Debug)]
enum Request {
    Refresh,
    /// Lazily fetch `/api/show` for one model — sent when the user opens the
    /// detail overlay, never on the poll tick.
    Show { engine_id: String, model: String },
}

pub struct InferenceHandle {
    rx: watch::Receiver<Arc<InferenceSnapshot>>,
    tx: mpsc::Sender<Request>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for InferenceHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl InferenceHandle {
    /// Cheap: clones an `Arc`, never blocks, never awaits.
    pub fn snapshot(&self) -> Arc<InferenceSnapshot> {
        self.rx.borrow().clone()
    }

    pub fn request_refresh(&self) {
        let _ = self.tx.try_send(Request::Refresh);
    }

    pub fn request_detail(&self, engine_id: &str, model: &str) {
        let _ = self.tx.try_send(Request::Show {
            engine_id: engine_id.to_string(),
            model: model.to_string(),
        });
    }
}

// ---------------------------------------------------------------------------

#[derive(Clone)]
enum EngineClient {
    Ollama(OllamaClient),
    Vllm(VllmClient),
    Unsupported,
}

struct EngineState {
    snapshot: EngineSnapshot,
    client: EngineClient,
    tags: Vec<ModelInstance>,
    tags_at: Option<Instant>,
    show: ShowCache,
    counters: Option<Counters>,
}

struct Probe {
    idx: usize,
    models: Result<Vec<ModelInstance>, ProbeError>,
    tags: Option<Result<Vec<ModelInstance>, ProbeError>>,
    metrics: Option<InferenceMetrics>,
    counters: Option<Counters>,
}

/// reqwest follows `HTTP_PROXY` by default, which routes
/// `http://localhost:11434` through a corporate proxy and hangs. Disabling it
/// is only safe when every configured endpoint is loopback.
fn all_loopback(engines: &[InferenceEngine]) -> bool {
    engines.iter().all(|e| {
        let u = e.url.to_ascii_lowercase();
        u.contains("://localhost") || u.contains("://127.0.0.1") || u.contains("://[::1]")
    })
}

fn build_client(cfg: &InferenceConfig, loopback_only: bool) -> reqwest::Client {
    let mut b = reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(cfg.connect_timeout_ms))
        .timeout(Duration::from_millis(cfg.timeout_ms.max(cfg.show_timeout_ms)))
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(4)
        .user_agent(concat!("pgxtop/", env!("CARGO_PKG_VERSION")));
    if loopback_only {
        b = b.no_proxy();
    }
    b.build().unwrap_or_else(|_| reqwest::Client::new())
}

pub fn spawn(engines: Vec<InferenceEngine>, cfg: InferenceConfig) -> InferenceHandle {
    let http = build_client(&cfg, all_loopback(&engines));
    let timeout = Duration::from_millis(cfg.timeout_ms);
    let show_timeout = Duration::from_millis(cfg.show_timeout_ms);

    let states: Vec<EngineState> = engines
        .into_iter()
        .map(|engine| {
            let client = match engine.engine_type {
                EngineType::Ollama => EngineClient::Ollama(OllamaClient::new(
                    http.clone(),
                    engine.url.clone(),
                    engine.id.clone(),
                    timeout,
                    show_timeout,
                )),
                EngineType::Vllm => EngineClient::Vllm(VllmClient::new(
                    http.clone(),
                    engine.url.clone(),
                    engine.id.clone(),
                    timeout,
                )),
                _ => EngineClient::Unsupported,
            };
            EngineState {
                snapshot: EngineSnapshot {
                    engine,
                    models: Vec::new(),
                    metrics: None,
                    last_ok: None,
                    consecutive_failures: 0,
                    last_error: None,
                },
                client,
                tags: Vec::new(),
                tags_at: None,
                show: ShowCache::default(),
                counters: None,
            }
        })
        .collect();

    let initial = Arc::new(InferenceSnapshot {
        engines: states.iter().map(|s| s.snapshot.clone()).collect(),
        generation: 0,
        stale_after: Duration::from_millis(cfg.stale_after_ms),
    });

    let (watch_tx, watch_rx) = watch::channel(initial);
    let (req_tx, req_rx) = mpsc::channel(16);

    let task = tokio::spawn(run(states, cfg, watch_tx, req_rx));

    InferenceHandle {
        rx: watch_rx,
        tx: req_tx,
        task,
    }
}

async fn run(
    mut states: Vec<EngineState>,
    cfg: InferenceConfig,
    watch_tx: watch::Sender<Arc<InferenceSnapshot>>,
    mut req_rx: mpsc::Receiver<Request>,
) {
    let interval = Duration::from_millis(cfg.refresh_ms.max(100));
    let mut generation = 0u64;
    let mut next = tokio::time::Instant::now();

    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(next) => {}
            req = req_rx.recv() => {
                match req {
                    None => return,
                    Some(Request::Refresh) => {}
                    Some(Request::Show { engine_id, model }) => {
                        fetch_detail(&mut states, &engine_id, &model, &cfg).await;
                        generation += 1;
                        publish(&states, &cfg, generation, &watch_tx);
                        continue;
                    }
                }
            }
        }
        next = tokio::time::Instant::now() + interval;

        poll_all(&mut states, &cfg).await;
        generation += 1;
        publish(&states, &cfg, generation, &watch_tx);
    }
}

async fn poll_all(states: &mut [EngineState], cfg: &InferenceConfig) {
    let now = Instant::now();
    let tags_interval = Duration::from_millis(cfg.tags_refresh_ms);

    let mut set: JoinSet<Probe> = JoinSet::new();
    for (idx, st) in states.iter().enumerate() {
        let client = st.client.clone();
        let want_tags = cfg.include_installed
            && st
                .tags_at
                .is_none_or(|t| now.duration_since(t) >= tags_interval);
        let prev_counters = st.counters;

        // All engines are probed concurrently, so the cycle costs
        // max(timeout) rather than sum(timeout).
        set.spawn(async move { probe(idx, client, want_tags, prev_counters).await });
    }

    let mut results: Vec<Probe> = Vec::new();
    while let Some(joined) = set.join_next().await {
        if let Ok(p) = joined {
            results.push(p);
        }
    }

    for p in results {
        apply(&mut states[p.idx], p, cfg);
    }
}

async fn probe(
    idx: usize,
    client: EngineClient,
    want_tags: bool,
    prev_counters: Option<Counters>,
) -> Probe {
    match client {
        EngineClient::Ollama(c) => {
            let models = c.fetch_ps().await;
            let tags = if want_tags && models.is_ok() {
                Some(c.fetch_tags().await)
            } else {
                None
            };
            Probe { idx, models, tags, metrics: None, counters: None }
        }
        EngineClient::Vllm(c) => {
            let models = c.fetch_models().await;
            let (metrics, counters) = match c.fetch_metrics(prev_counters.as_ref()).await {
                Ok((m, c)) => (Some(m), Some(c)),
                Err(_) => (None, prev_counters),
            };
            Probe { idx, models, tags: None, metrics, counters }
        }
        EngineClient::Unsupported => Probe {
            idx,
            models: Err(ProbeError::Transport("engine type not implemented".into())),
            tags: None,
            metrics: None,
            counters: None,
        },
    }
}

fn apply(st: &mut EngineState, p: Probe, cfg: &InferenceConfig) {
    let now = Instant::now();

    if let Some(Ok(tags)) = p.tags {
        st.tags = tags;
        st.tags_at = Some(now);
    }
    if let Some(c) = p.counters {
        st.counters = Some(c);
    }

    match p.models {
        Ok(loaded) => {
            let merged = if cfg.include_installed && !st.tags.is_empty() {
                crate::engines::ollama::map::merge(loaded, st.tags.clone())
            } else {
                loaded
            };
            st.snapshot.models = attach_details(merged, &st.show, now);
            st.snapshot.metrics = p.metrics.or(st.snapshot.metrics.take());
            st.snapshot.last_ok = Some(now);
            st.snapshot.consecutive_failures = 0;
            st.snapshot.last_error = None;
            st.snapshot.engine.status = EngineStatus::Connected;

            // Keep the /api/show cache from growing on a box that cycles
            // through many models.
            let live: HashSet<ShowKey> = st
                .snapshot
                .models
                .iter()
                .map(|m| ShowKey::new(&m.name, m.digest.as_deref()))
                .collect();
            st.show.retain_live(&live);
        }
        Err(e) => {
            st.snapshot.consecutive_failures = st.snapshot.consecutive_failures.saturating_add(1);
            st.snapshot.last_error = Some(e);
            st.snapshot.engine.status = EngineStatus::Unavailable;
            // models / last_ok are deliberately left untouched: the UI keeps
            // showing the last known state, dimmed, with its age.
            let drop_after = Duration::from_millis(cfg.drop_after_ms);
            if st.snapshot.last_ok.is_none_or(|t| t.elapsed() > drop_after) {
                st.snapshot.models.clear();
                st.snapshot.metrics = None;
            }
        }
    }
}

fn attach_details(
    mut models: Vec<ModelInstance>,
    cache: &ShowCache,
    now: Instant,
) -> Vec<ModelInstance> {
    for m in &mut models {
        let key = ShowKey::new(&m.name, m.digest.as_deref());
        m.detail = cache.get(&key, now);
    }
    models
}

async fn fetch_detail(
    states: &mut [EngineState],
    engine_id: &str,
    model: &str,
    cfg: &InferenceConfig,
) {
    if !cfg.show_details {
        return;
    }
    let Some(st) = states.iter_mut().find(|s| s.snapshot.engine.id == engine_id) else {
        return;
    };
    let EngineClient::Ollama(client) = st.client.clone() else {
        return;
    };

    let digest = st
        .snapshot
        .models
        .iter()
        .find(|m| m.name == model)
        .and_then(|m| m.digest.clone());
    let key = ShowKey::new(model, digest.as_deref());

    let now = Instant::now();
    if st.show.lookup(&key, now) != Lookup::Missing {
        return;
    }
    st.show.mark_in_flight(key.clone());

    match client.fetch_show(model).await {
        Ok(detail) => st.show.insert_hit(key, detail, Instant::now()),
        Err(e) => {
            tracing::debug!(target: "pgxtop::inference", "/api/show failed for {model}: {e}");
            st.show.insert_miss(key, Instant::now());
        }
    }

    let now = Instant::now();
    let models = std::mem::take(&mut st.snapshot.models);
    st.snapshot.models = attach_details(models, &st.show, now);
}

fn publish(
    states: &[EngineState],
    cfg: &InferenceConfig,
    generation: u64,
    tx: &watch::Sender<Arc<InferenceSnapshot>>,
) {
    let snap = InferenceSnapshot {
        engines: states.iter().map(|s| s.snapshot.clone()).collect(),
        generation,
        stale_after: Duration::from_millis(cfg.stale_after_ms),
    };
    let _ = tx.send(Arc::new(snap));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::inference::ModelStatus;

    fn engine(id: &str, url: &str) -> InferenceEngine {
        InferenceEngine {
            id: id.into(),
            name: id.into(),
            engine_type: EngineType::Ollama,
            url: url.into(),
            status: EngineStatus::Connecting,
        }
    }

    fn model(name: &str) -> ModelInstance {
        ModelInstance {
            id: format!("ollama/{name}"),
            name: name.into(),
            engine_id: "ollama".into(),
            status: ModelStatus::Loaded,
            ..Default::default()
        }
    }

    fn state() -> EngineState {
        EngineState {
            snapshot: EngineSnapshot {
                engine: engine("ollama", "http://localhost:11434"),
                models: Vec::new(),
                metrics: None,
                last_ok: None,
                consecutive_failures: 0,
                last_error: None,
            },
            client: EngineClient::Unsupported,
            tags: Vec::new(),
            tags_at: None,
            show: ShowCache::default(),
            counters: None,
        }
    }

    fn probe_ok(models: Vec<ModelInstance>) -> Probe {
        Probe { idx: 0, models: Ok(models), tags: None, metrics: None, counters: None }
    }

    fn probe_err() -> Probe {
        Probe {
            idx: 0,
            models: Err(ProbeError::Transport("connection refused".into())),
            tags: None,
            metrics: None,
            counters: None,
        }
    }

    /// The core of the fix: a failed probe must not blank the table.
    #[test]
    fn a_failed_probe_retains_the_last_known_models() {
        let cfg = InferenceConfig::default();
        let mut st = state();

        apply(&mut st, probe_ok(vec![model("example-moe:q8")]), &cfg);
        assert_eq!(st.snapshot.models.len(), 1);
        assert_eq!(st.snapshot.engine.status, EngineStatus::Connected);
        assert!(st.snapshot.last_ok.is_some());

        apply(&mut st, probe_err(), &cfg);
        assert_eq!(st.snapshot.models.len(), 1, "models must be retained");
        assert_eq!(st.snapshot.engine.status, EngineStatus::Unavailable);
        assert_eq!(st.snapshot.consecutive_failures, 1);
        assert_eq!(st.snapshot.last_error.as_ref().unwrap().short(), "connection refused");

        apply(&mut st, probe_err(), &cfg);
        assert_eq!(st.snapshot.consecutive_failures, 2);

        // Recovery clears the error and refreshes the list.
        apply(&mut st, probe_ok(vec![model("a"), model("b")]), &cfg);
        assert_eq!(st.snapshot.models.len(), 2);
        assert_eq!(st.snapshot.consecutive_failures, 0);
        assert!(st.snapshot.last_error.is_none());
    }

    /// ...but a genuinely dead engine must not show phantom models forever.
    #[test]
    fn models_are_dropped_after_the_grace_period() {
        let cfg = InferenceConfig { drop_after_ms: 0, ..Default::default() };
        let mut st = state();
        apply(&mut st, probe_ok(vec![model("example-moe:q8")]), &cfg);
        apply(&mut st, probe_err(), &cfg);
        assert!(st.snapshot.models.is_empty());
    }

    /// An engine that was never reachable is "unavailable", not "stale".
    #[test]
    fn never_reached_engine_is_not_reported_as_stale() {
        let st = state();
        assert!(!st.snapshot.is_stale(Duration::from_millis(1)));
        assert_eq!(st.snapshot.age(), None);
    }

    #[test]
    fn proxy_is_only_bypassed_for_loopback_endpoints() {
        assert!(all_loopback(&[
            engine("a", "http://localhost:11434"),
            engine("b", "http://127.0.0.1:8888"),
        ]));
        assert!(!all_loopback(&[
            engine("a", "http://localhost:11434"),
            engine("b", "http://10.0.0.5:8888"),
        ]));
    }
}
