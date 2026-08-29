use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::Terminal;

use crate::collectors::correlate::{self, ManifestIndex};
use crate::collectors::inference::{self, InferenceHandle, InferenceSnapshot};
use crate::collectors::{nvml, Collectors};
use crate::config::Config;
use crate::models::{
    GpuHistory, GpuInfo, GpuMetrics, GpuProcess, GpuSummary, InferenceEngine, InferenceHistory,
    ProcessInfo, SystemHistory, SystemInfo, SystemMetrics,
};
use crate::ui::state::{self, ModelRow, ModelsUiState, Overlay};

const HISTORY_POINTS: usize = 300;

pub struct App {
    pub config: Config,
    pub collectors: Collectors,
    pub running: bool,
    pub current_view: View,
    pub overlay: Overlay,
    pub paused: bool,
    pub last_refresh: Instant,
    force_refresh: bool,

    // GPU
    pub gpu_info: Vec<GpuInfo>,
    pub gpu_metrics: Vec<GpuMetrics>,
    pub gpu_processes: Vec<GpuProcess>,
    pub gpu_history: Vec<GpuHistory>,
    manifests: ManifestIndex,

    // System
    pub system_info: Option<SystemInfo>,
    pub system_metrics: Option<SystemMetrics>,
    pub system_processes: Vec<ProcessInfo>,
    pub system_history: SystemHistory,

    // Inference — one snapshot instead of three parallel vectors.
    inference: InferenceHandle,
    pub inference_snapshot: Arc<InferenceSnapshot>,
    pub inference_history: InferenceHistory,
    last_generation: u64,

    // Models view
    pub model_rows: Vec<ModelRow>,
    pub models_ui: ModelsUiState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Overview,
    Gpu,
    Llm,
    Processes,
    System,
    Network,
}

/// Every key `handle_key` actually acts on, in the same spelling the footer
/// and help overlay use. `ui::keys` asserts the two lists agree.
#[cfg(test)]
pub fn handled_keys() -> Vec<&'static str> {
    vec![
        "1-6",
        "r",
        "p",
        "?",
        "q",
        "Esc",
        "↑↓/jk",
        "PgUp/PgDn",
        "g/G",
        "Enter",
        "s",
        "S",
        "t",
        "Tab",
    ]
}

impl App {
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let collectors = Collectors::new();

        let mut engines: Vec<InferenceEngine> = Vec::new();
        if config.ollama.enabled {
            engines.push(InferenceEngine {
                id: "ollama".to_string(),
                name: "Ollama".to_string(),
                engine_type: crate::engines::EngineType::Ollama,
                url: config.ollama.url.clone(),
                status: crate::engines::EngineStatus::Connecting,
            });
        }
        for vllm in &config.vllm {
            engines.push(InferenceEngine {
                id: format!("vllm-{}", vllm.name),
                name: format!("vLLM ({})", vllm.name),
                engine_type: crate::engines::EngineType::Vllm,
                url: vllm.url.clone(),
                status: crate::engines::EngineStatus::Connecting,
            });
        }

        let multi_engine = engines.len() > 1;
        let handle = inference::spawn(engines, config.inference_config());
        let snapshot = handle.snapshot();

        Ok(Self {
            config,
            collectors,
            running: true,
            current_view: View::Overview,
            overlay: Overlay::None,
            paused: false,
            last_refresh: Instant::now(),
            force_refresh: true,
            gpu_info: Vec::new(),
            gpu_metrics: Vec::new(),
            gpu_processes: Vec::new(),
            gpu_history: Vec::new(),
            manifests: ManifestIndex::discover(),
            system_info: None,
            system_metrics: None,
            system_processes: Vec::new(),
            system_history: SystemHistory::new(HISTORY_POINTS),
            inference: handle,
            inference_snapshot: snapshot,
            inference_history: InferenceHistory::new(HISTORY_POINTS),
            last_generation: 0,
            model_rows: Vec::new(),
            models_ui: ModelsUiState {
                multi_engine,
                ..Default::default()
            },
        })
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend)?;

        self.init_terminal()?;
        // The loop is extracted so an error inside it cannot skip the restore.
        let result = self.event_loop(&mut terminal);
        let _ = self.restore_terminal();
        result
    }

    fn event_loop<B: ratatui::backend::Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
    ) -> anyhow::Result<()> {
        // Entering the alternate screen does not blank it, and ratatui's first
        // draw only emits cells that differ from its previous buffer — which
        // starts out empty. Every blank cell in the first frame is therefore
        // skipped, leaving whatever the shell had printed there visible
        // underneath the panels. Clearing makes "empty" an accurate model of
        // the screen, so the diff is correct from the first frame on.
        terminal.clear()?;

        loop {
            let due = self.last_refresh.elapsed() >= Duration::from_millis(self.config.refresh_ms);
            if self.force_refresh || (!self.paused && due) {
                self.force_refresh = false;
                self.refresh();
                self.last_refresh = Instant::now();
            }

            crate::ui::draw(terminal, self)?;

            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key),
                    // Redraw immediately so a resize is not shown stale for a
                    // whole poll interval.
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }

            if !self.running {
                return Ok(());
            }
        }
    }

    /// Fully synchronous: the inference data is read from the background
    /// poller's last published snapshot, never awaited here.
    fn refresh(&mut self) {
        // Order matters: `refresh_gpu` resolves unified memory from the host
        // figures and correlates processes against the model list, so both
        // must already be current for this tick.
        self.refresh_system();
        self.refresh_inference();
        self.refresh_gpu();
        self.rebuild_model_rows();
    }

    fn refresh_gpu(&mut self) {
        let samples = self.collectors.nvml.collect();

        if self.gpu_history.len() < samples.len() {
            self.gpu_history
                .resize_with(samples.len(), || GpuHistory::new(HISTORY_POINTS));
        }

        let (host_used, host_total) = match (&self.system_metrics, &self.system_info) {
            (Some(m), Some(i)) => (m.used_memory, i.total_memory),
            _ => (0, 0),
        };

        self.gpu_info.clear();
        self.gpu_metrics.clear();
        self.gpu_processes.clear();

        for (i, mut sample) in samples.into_iter().enumerate() {
            // On unified-memory hardware NVML reports no frame buffer, so the
            // host figures plus per-process residency stand in for it.
            nvml::resolve_memory(
                &mut sample.metrics,
                &sample.processes,
                host_used,
                host_total,
            );
            if let Some(history) = self.gpu_history.get_mut(i) {
                history.push(&sample.metrics);
            }
            self.gpu_info.push(sample.info);
            self.gpu_metrics.push(sample.metrics);
            self.gpu_processes.extend(sample.processes);
        }

        let models: Vec<_> = self.inference_snapshot.models().cloned().collect();
        correlate::correlate(&mut self.gpu_processes, &models, &self.manifests);
        self.gpu_processes
            .sort_by_key(|p| std::cmp::Reverse(p.used_memory.unwrap_or(0)));
    }

    fn refresh_system(&mut self) {
        let (info, metrics, mut procs) = self.collectors.system.collect();
        self.system_history.push(&metrics, info.total_memory);
        self.system_info = Some(info);
        self.system_metrics = Some(metrics);

        // Sorted here rather than in the renderer, which used to clone and
        // re-sort the whole list on every frame.
        procs.sort_by(|a, b| {
            b.cpu_usage
                .partial_cmp(&a.cpu_usage)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        procs.truncate(200);
        self.system_processes = procs;
    }

    fn refresh_inference(&mut self) {
        let snap = self.inference.snapshot();
        // Guard against pushing the same sample repeatedly: the poller runs at
        // 2 s while the render loop refreshes at 500 ms.
        if snap.generation != self.last_generation {
            self.last_generation = snap.generation;
            for e in &snap.engines {
                if let Some(m) = &e.metrics {
                    self.inference_history.push(m);
                }
            }
        }
        self.models_ui.any_engine_connected = snap.any_connected();
        self.inference_snapshot = snap;
    }

    pub fn rebuild_model_rows(&mut self) {
        self.model_rows = state::build_rows(
            &self.inference_snapshot,
            self.models_ui.sort,
            self.models_ui.sort_desc,
            self.models_ui.filter_engine.as_deref(),
        );
        self.models_ui.reconcile(&self.model_rows);
    }

    pub fn gpu_summaries(&self) -> Vec<GpuSummary> {
        self.gpu_info
            .iter()
            .zip(self.gpu_metrics.iter())
            .map(|(i, m)| GpuSummary::from_parts(i, m))
            .collect()
    }

    fn switch_view(&mut self, c: char) {
        self.current_view = match c {
            '1' => View::Overview,
            '2' => View::Gpu,
            '3' => View::Llm,
            '4' => View::Processes,
            '5' => View::System,
            '6' => View::Network,
            _ => self.current_view,
        };
    }

    fn cycle_engine_filter(&mut self) {
        let ids: Vec<String> = self
            .inference_snapshot
            .engines
            .iter()
            .map(|e| e.engine.id.clone())
            .collect();
        if ids.is_empty() {
            return;
        }
        self.models_ui.filter_engine = match &self.models_ui.filter_engine {
            None => ids.first().cloned(),
            Some(cur) => match ids.iter().position(|i| i == cur) {
                Some(p) if p + 1 < ids.len() => Some(ids[p + 1].clone()),
                // Wraps back to "all engines".
                _ => None,
            },
        };
    }

    /// Asks the poller to fetch `/api/show` for the selected model. Cheap and
    /// cached, so repeated opens cost nothing.
    fn request_selected_detail(&self) {
        if let Some(row) = self.models_ui.selected(&self.model_rows) {
            if row.detail.is_none() {
                self.inference.request_detail(&row.engine_id, &row.name);
            }
        }
    }

    fn handle_key(&mut self, key: event::KeyEvent) {
        // Ctrl-C: raw mode swallows SIGINT, so it has to be handled here.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.running = false;
            return;
        }

        // An open overlay swallows everything except closing it and switching
        // views — otherwise `q` behind a modal would quit the app.
        if self.overlay != Overlay::None {
            match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('?') => {
                    self.overlay = Overlay::None;
                }
                KeyCode::Char(c @ '1'..='6') => {
                    self.overlay = Overlay::None;
                    self.switch_view(c);
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.running = false,
            KeyCode::Char(c @ '1'..='6') => self.switch_view(c),
            KeyCode::Char('r') => {
                // Used to *delay* the next refresh by resetting the timer.
                self.force_refresh = true;
                self.inference.request_refresh();
            }
            KeyCode::Char('p') => self.paused = !self.paused,
            KeyCode::Char('?') => self.overlay = Overlay::Help,
            KeyCode::Esc => {}
            _ if self.current_view == View::Llm => self.handle_models_key(key),
            _ => {}
        }
    }

    fn handle_models_key(&mut self, key: event::KeyEvent) {
        // Destructured so the nav helpers can borrow the rows and the UI state
        // at the same time.
        let App {
            models_ui,
            model_rows,
            ..
        } = self;
        let page = models_ui.page();

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => models_ui.move_by(-1, model_rows),
            KeyCode::Down | KeyCode::Char('j') => models_ui.move_by(1, model_rows),
            KeyCode::PageUp => models_ui.move_by(-page, model_rows),
            KeyCode::PageDown => models_ui.move_by(page, model_rows),
            KeyCode::Home | KeyCode::Char('g') => models_ui.select_index(0, model_rows),
            KeyCode::End | KeyCode::Char('G') => {
                models_ui.select_index(model_rows.len().saturating_sub(1), model_rows)
            }
            KeyCode::Enter => {
                if models_ui.selected(model_rows).is_some() {
                    self.overlay = Overlay::Detail;
                    self.request_selected_detail();
                }
            }
            KeyCode::Char('s') => {
                self.models_ui.sort = self.models_ui.sort.next();
                self.rebuild_model_rows();
            }
            KeyCode::Char('S') => {
                self.models_ui.sort_desc = !self.models_ui.sort_desc;
                self.rebuild_model_rows();
            }
            KeyCode::Char('t') => self.models_ui.show_history = !self.models_ui.show_history,
            KeyCode::Tab => {
                self.cycle_engine_filter();
                self.rebuild_model_rows();
            }
            _ => {}
        }
    }

    fn init_terminal(&self) -> anyhow::Result<()> {
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
        Ok(())
    }

    fn restore_terminal(&self) -> anyhow::Result<()> {
        crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;
        crossterm::terminal::disable_raw_mode()?;
        Ok(())
    }
}

/// Restores the terminal on a panic.
///
/// A `Drop` guard would not work: the release profile sets `panic = "abort"`,
/// so nothing unwinds — but the hook still runs before the abort.
pub fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            // ratatui hides the cursor while drawing and restores it on drop,
            // which an abort skips — so restore it here too.
            crossterm::cursor::Show,
        );
        let _ = crossterm::terminal::disable_raw_mode();
        original(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::state::ModelSort;

    #[test]
    fn the_default_sort_is_by_name() {
        assert_eq!(ModelSort::default(), ModelSort::Name);
    }

    #[test]
    fn handled_keys_has_no_duplicates() {
        let mut keys = handled_keys();
        let before = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), before);
    }
}
